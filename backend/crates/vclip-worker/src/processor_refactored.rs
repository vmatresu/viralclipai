//! Refactored video processing architecture.
//!
//! This module replaces the monolithic processor.rs with a modular, testable,
//! and maintainable architecture using the new style processor framework.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use futures::future::join_all;
use tokio::sync::Semaphore;
use tracing::info;

use vclip_firestore::{types::ToFirestoreValue, ClipRepository, FirestoreClient};
use vclip_media::{
    download_video,
    core::{ProcessingRequest, ProcessingContext as MediaProcessingContext, StyleProcessorRegistry, MetricsCollector, SecurityContext},
    styles::StyleProcessorFactory as MediaStyleProcessorFactory,
};
use vclip_models::{AspectRatio, ClipMetadata, ClipTask, CropMode, EncodingConfig, JobId, Style, VideoId, VideoMetadata};
use vclip_queue::{ProcessVideoJob, ProgressChannel};
use vclip_storage::R2Client;

use crate::config::WorkerConfig;
use crate::error::{WorkerError, WorkerResult};
use crate::gemini::GeminiClient;

/// Default prompt for AI analysis when no custom prompt is provided.
const DEFAULT_PROMPT: &str = r#"You are a viral content expert. Analyze this video transcript and identify the most engaging, viral-worthy moments that would work well as short-form clips for TikTok, YouTube Shorts, or Instagram Reels.

For each highlight, provide:
- A catchy title
- Start and end timestamps
- Why this moment is viral-worthy
- The hook category (e.g., "controversial", "emotional", "educational", "funny")

Focus on moments with:
- Strong emotional hooks
- Surprising revelations
- Controversial statements
- Actionable advice
- Memorable quotes"#;

/// Try to load a custom prompt from a file.
fn load_prompt_from_file() -> Option<String> {
    let prompt_path = std::env::var("PROMPT_FILE").ok()?;
    std::fs::read_to_string(&prompt_path).ok()
}

/// Enhanced processing context with new architecture components.
pub struct EnhancedProcessingContext {
    pub config: WorkerConfig,
    pub storage: R2Client,
    pub firestore: FirestoreClient,
    pub progress: ProgressChannel,
    pub ffmpeg_semaphore: Arc<Semaphore>,

    // New architecture components
    pub style_registry: Arc<StyleProcessorRegistry>,
    pub metrics: Arc<MetricsCollector>,
    pub security: Arc<SecurityContext>,
}

impl EnhancedProcessingContext {
    /// Create a new enhanced processing context.
    pub async fn new(config: WorkerConfig) -> WorkerResult<Self> {
        let storage = R2Client::from_env()
            .await
            .map_err(|e| WorkerError::Storage(e))?;

        let firestore = FirestoreClient::from_env()
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let progress =
            ProgressChannel::new(&redis_url).map_err(|e| WorkerError::Queue(e))?;

        let ffmpeg_semaphore = Arc::new(Semaphore::new(config.max_ffmpeg_processes));

        // Initialize new architecture components
        let metrics = Arc::new(MetricsCollector::new());
        let security = Arc::new(SecurityContext::new());
        let style_factory = MediaStyleProcessorFactory::new();
        let mut style_registry = StyleProcessorRegistry::new();
        style_registry.register_factory(Arc::new(style_factory));

        Ok(Self {
            config,
            storage,
            firestore,
            progress,
            ffmpeg_semaphore,
            style_registry: Arc::new(style_registry),
            metrics,
            security,
        })
    }

}

/// Video processing coordinator using the new architecture.
#[derive(Clone)]
pub struct VideoProcessor {
    gemini_client: Arc<GeminiClient>,
}

impl VideoProcessor {
    /// Create a new video processor.
    pub fn new() -> WorkerResult<Self> {
        Ok(Self {
            gemini_client: Arc::new(GeminiClient::new()?),
        })
    }

    /// Process a video job using the enhanced architecture.
    pub async fn process_video_job(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
    ) -> WorkerResult<()> {
        let job_logger = JobLogger::new(&job.job_id, "video_processing");

        job_logger.log_start("Starting video processing job");

        // Create work directory
        let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
        tokio::fs::create_dir_all(&work_dir).await?;

        ctx.progress
            .log(&job.job_id, "Starting video processing...")
            .await
            .ok();
        ctx.progress.progress(&job.job_id, 5).await.ok();

        // Get transcript and video metadata
        let transcript_data = self.fetch_transcript_and_metadata(ctx, job).await?;
        ctx.progress.progress(&job.job_id, 10).await.ok();

        // Download video and analyze in parallel
        let analysis_data = self.download_and_analyze(ctx, job, &work_dir, &transcript_data).await?;
        ctx.progress.progress(&job.job_id, 35).await.ok();

        // Store video metadata
        self.store_video_metadata(ctx, job, &transcript_data, &analysis_data).await?;

        // Generate and process clips
        let clip_results = self.process_clips(ctx, job, &work_dir, &analysis_data).await?;

        // Finalize video
        self.finalize_video(ctx, job, clip_results.total_processed as u32).await?;

        job_logger.log_completion(&format!("Processed {} clips successfully", clip_results.completed_count));

        Ok(())
    }

    /// Fetch transcript and video metadata.
    async fn fetch_transcript_and_metadata(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
    ) -> WorkerResult<TranscriptData> {
        ctx.progress
            .log(&job.job_id, "Fetching video transcript...")
            .await
            .ok();

        let base_prompt = job.custom_prompt.as_deref()
            .map(|s| s.to_string())
            .or_else(|| load_prompt_from_file())
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

        let (real_video_title, canonical_video_url) = self.gemini_client.get_video_metadata(&job.video_url).await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to get video metadata: {}", e)))?;

        let transcript = self.gemini_client.get_transcript_only(&job.video_url, &PathBuf::from(&ctx.config.work_dir).join("temp")).await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to get transcript: {}", e)))?;

        Ok(TranscriptData {
            title: real_video_title,
            url: canonical_video_url,
            content: transcript,
            prompt: base_prompt,
        })
    }

    /// Download video and analyze content in parallel.
    async fn download_and_analyze(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        work_dir: &Path,
        transcript: &TranscriptData,
    ) -> WorkerResult<AnalysisData> {
        ctx.progress
            .log(&job.job_id, "Downloading video and analyzing with AI...")
            .await
            .ok();

        let video_file = work_dir.join("source.mp4");

        let (download_result, analysis_result) = tokio::join!(
            download_video(&job.video_url, &video_file),
            self.gemini_client.analyze_transcript(&transcript.prompt, &job.video_url, &transcript.content)
        );

        download_result?;
        let ai_response = analysis_result?;

        Ok(AnalysisData {
            video_file,
            highlights: ai_response,
        })
    }

    /// Store video metadata in Firestore.
    async fn store_video_metadata(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        transcript: &TranscriptData,
        analysis: &AnalysisData,
    ) -> WorkerResult<()> {
        let video_meta = VideoMetadata::new(
            job.video_id.clone(),
            &job.user_id,
            &transcript.url,
            &transcript.title,
        );

        let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

        // Store highlights
        let highlights_data = vclip_storage::HighlightsData {
            highlights: analysis.highlights.highlights.iter().map(|h| {
                vclip_storage::operations::HighlightEntry {
                    id: h.id,
                    title: h.title.clone(),
                    description: h.description.clone(),
                    start: h.start.clone(),
                    end: h.end.clone(),
                    duration: h.duration,
                    hook_category: h.hook_category.clone(),
                    reason: h.reason.clone(),
                }
            }).collect(),
            video_url: Some(transcript.url.clone()),
            video_title: Some(transcript.title.clone()),
            custom_prompt: job.custom_prompt.clone(),
        };

        ctx.storage
            .upload_highlights(&job.user_id, job.video_id.as_str(), &highlights_data)
            .await
            .map_err(|e| WorkerError::Storage(e))?;

        // Create/update video record
        if let Ok(Some(mut existing_video)) = video_repo.get(&job.video_id).await {
            existing_video.video_title = transcript.title.clone();
            existing_video.video_url = transcript.url.clone();
            existing_video.status = vclip_models::VideoStatus::Processing;

            let mut fields = HashMap::new();
            fields.insert("video_title".to_string(), transcript.title.clone().to_firestore_value());
            fields.insert("video_url".to_string(), transcript.url.clone().to_firestore_value());
            fields.insert("status".to_string(), vclip_models::VideoStatus::Processing.as_str().to_firestore_value());

            ctx.firestore
                .update_document(
                    &format!("users/{}/videos", job.user_id),
                    job.video_id.as_str(),
                    fields,
                    Some(vec!["video_title".to_string(), "video_url".to_string(), "status".to_string()]),
                )
                .await
                .ok();
        } else {
            video_repo
                .create(&video_meta)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }

        Ok(())
    }

    /// Process all clips using the new architecture.
    async fn process_clips(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        work_dir: &Path,
        analysis: &AnalysisData,
    ) -> WorkerResult<ClipProcessingResults> {
        let clip_tasks = generate_clip_tasks(&analysis.highlights, &job.styles, &job.crop_mode, &job.target_aspect);
        let total_clips = clip_tasks.len();

        ctx.progress
            .log(
                &job.job_id,
                format!("Generating {} clips from {} highlights...", total_clips, analysis.highlights.highlights.len()),
            )
            .await
            .ok();

        let clips_dir = work_dir.join("clips");
        tokio::fs::create_dir_all(&clips_dir).await?;

        // Group clips by scene for parallel processing
        let mut scene_groups: HashMap<u32, Vec<&ClipTask>> = HashMap::new();
        for task in &clip_tasks {
            scene_groups.entry(task.scene_id).or_default().push(task);
        }

        let mut scene_ids: Vec<u32> = scene_groups.keys().copied().collect();
        scene_ids.sort();

        let mut completed_clips = 0u32;
        let mut processed_count = 0usize;

        // Process each scene
        for scene_id in scene_ids {
            let scene_tasks = scene_groups.get(&scene_id).unwrap();
            let scene_results = self.process_scene(ctx, job, &clips_dir, &analysis.video_file, scene_tasks, total_clips).await?;

            processed_count += scene_results.processed;
            completed_clips += scene_results.completed;

            let progress = 40 + (processed_count * 55 / total_clips) as u32;
            ctx.progress.progress(&job.job_id, progress as u8).await.ok();
        }

        Ok(ClipProcessingResults {
            total_processed: processed_count,
            completed_count: completed_clips,
        })
    }

    /// Process a single scene with parallel style processing.
    async fn process_scene(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        clips_dir: &Path,
        video_file: &Path,
        scene_tasks: &[&ClipTask],
        total_clips: usize,
    ) -> WorkerResult<SceneProcessingResults> {
        ctx.progress
            .log(
                &job.job_id,
                format!(
                    "Processing scene {} ({} styles in parallel)...",
                    scene_tasks[0].scene_id,
                    scene_tasks.len()
                ),
            )
            .await
            .ok();

        let futures: Vec<_> = scene_tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| {
                let ctx = ctx;
                let job_id = &job.job_id;
                let video_id = &job.video_id;
                let user_id = &job.user_id;
                let video_file = video_file;
                let clips_dir = clips_dir;

                async move {
                    self.process_single_clip(
                        ctx,
                        job_id,
                        video_id,
                        user_id,
                        video_file,
                        clips_dir,
                        task,
                        idx,
                        total_clips,
                    )
                    .await
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut processed = 0;
        let mut completed = 0;

        for result in results {
            processed += 1;
            match result {
                Ok(_) => completed += 1,
                Err(e) => {
                    ctx.progress
                        .log(&job.job_id, format!("Failed to process clip: {}", e))
                        .await
                        .ok();
                }
            }
        }

        Ok(SceneProcessingResults { processed, completed })
    }

    /// Process a single clip using the new architecture.
    async fn process_single_clip(
        &self,
        ctx: &EnhancedProcessingContext,
        job_id: &JobId,
        video_id: &VideoId,
        user_id: &str,
        video_file: &Path,
        clips_dir: &Path,
        task: &ClipTask,
        clip_index: usize,
        total_clips: usize,
    ) -> WorkerResult<()> {
        // Acquire semaphore for resource control
        let _permit = ctx.ffmpeg_semaphore.acquire().await
            .map_err(|_| WorkerError::job_failed("Failed to acquire FFmpeg permit"))?;

        let filename = task.output_filename();
        let output_path = clips_dir.join(&filename);

        info!(
            "Processing clip {}/{}: {}",
            clip_index + 1,
            total_clips,
            filename
        );

        // Create processing request
        let request = ProcessingRequest::new(
            task.clone(),
            video_file,
            &output_path,
            EncodingConfig::default(), // Will be overridden by style processor
            job_id.to_string(),
            user_id.to_string(),
        )?;

        // Create processing context
        let proc_ctx = MediaProcessingContext::new(
            request.request_id.clone(),
            request.user_id.clone(),
            clips_dir,
            ctx.ffmpeg_semaphore.clone(),
            ctx.metrics.clone(),
            ctx.security.clone(),
        );

        // Get style processor and process
        let processor = ctx.style_registry.get_processor(task.style).await?;
        let result = processor.process(request, proc_ctx).await?;

        // Upload to storage
        let r2_key = ctx
            .storage
            .upload_clip(&result.output_path, user_id, video_id.as_str(), &filename)
            .await
            .map_err(|e| WorkerError::Storage(e))?;

        let thumb_key = if let Some(thumb_path) = &result.thumbnail_path {
            let thumb_filename = filename.replace(".mp4", ".jpg");
            Some(
                ctx.storage
                    .upload_clip(thumb_path, user_id, video_id.as_str(), &thumb_filename)
                    .await
                    .map_err(|e| WorkerError::Storage(e))?,
            )
        } else {
            None
        };

        // Emit progress
        ctx.progress
            .clip_uploaded(job_id, video_id.as_str(), clip_index as u32 + 1, total_clips as u32)
            .await
            .ok();

        // Create clip metadata
        let clip_meta = ClipMetadata {
            clip_id: format!("{}_{}_{}", video_id, task.scene_id, task.style),
            video_id: video_id.clone(),
            user_id: user_id.to_string(),
            scene_id: task.scene_id,
            scene_title: task.scene_title.clone(),
            scene_description: task.scene_description.clone(),
            filename,
            style: task.style.to_string(),
            priority: task.priority,
            start_time: task.start.clone(),
            end_time: task.end.clone(),
            duration_seconds: result.duration_seconds,
            file_size_bytes: result.file_size_bytes,
            file_size_mb: result.file_size_bytes as f64 / (1024.0 * 1024.0),
            has_thumbnail: result.thumbnail_path.is_some(),
            r2_key,
            thumbnail_r2_key: thumb_key,
            status: vclip_models::ClipStatus::Completed,
            created_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            created_by: user_id.to_string(),
        };

        // Save clip metadata to Firestore
        let clip_repo = ClipRepository::new(
            ctx.firestore.clone(),
            user_id,
            video_id.clone(),
        );
        if let Err(e) = clip_repo.create(&clip_meta).await {
            tracing::warn!("Failed to save clip metadata to Firestore: {}", e);
        }

        Ok(())
    }

    /// Finalize video processing.
    async fn finalize_video(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        completed_clips: u32,
    ) -> WorkerResult<()> {
        let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

        match video_repo.complete(&job.video_id, completed_clips).await {
            Ok(_) => {
                info!("Successfully marked video {} as completed with {} clips", job.video_id, completed_clips);
            }
            Err(e) => {
                tracing::error!("Failed to mark video {} as completed: {}", job.video_id, e);
                return Err(WorkerError::Firestore(e));
            }
        }

        ctx.progress.progress(&job.job_id, 100).await.ok();
        ctx.progress
            .done(&job.job_id, job.video_id.as_str())
            .await
            .ok();

        info!("Completed video job: {} ({} clips)", job.job_id, completed_clips);
        Ok(())
    }
}

/// Data structures for processing pipeline.
struct TranscriptData {
    title: String,
    url: String,
    content: String,
    prompt: String,
}

struct AnalysisData {
    video_file: PathBuf,
    highlights: crate::gemini::HighlightsResponse,
}

struct ClipProcessingResults {
    total_processed: usize,
    completed_count: u32,
}

struct SceneProcessingResults {
    processed: usize,
    completed: u32,
}

/// Job logger for structured logging.
struct JobLogger {
    job_id: String,
    operation: String,
}

impl JobLogger {
    fn new(job_id: &JobId, operation: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            operation: operation.to_string(),
        }
    }

    fn log_start(&self, message: &str) {
        tracing::info!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job started: {}", message
        );
    }

    fn log_completion(&self, message: &str) {
        tracing::info!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job completed: {}", message
        );
    }
}

/// Generate clip tasks from highlights and styles.
///
/// Creates one ClipTask per (highlight, style) combination.
fn generate_clip_tasks(
    highlights: &crate::gemini::HighlightsResponse,
    styles: &[Style],
    crop_mode: &CropMode,
    target_aspect: &AspectRatio,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in &highlights.highlights {
        for style in styles {
            let task = ClipTask {
                scene_id: highlight.id,
                scene_title: sanitize_title(&highlight.title),
                scene_description: highlight.description.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: *style,
                crop_mode: *crop_mode,
                target_aspect: *target_aspect,
                priority: highlight.id, // Use highlight ID as priority
                pad_before: 0.0,
                pad_after: 0.0,
            };
            tasks.push(task);
        }
    }

    tasks
}

/// Sanitize a title for use in filenames.
fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .replace(' ', "_")
        .chars()
        .take(50)
        .collect()
}
