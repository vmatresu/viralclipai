//! Video processing orchestration.
//!
//! Modular, testable coordinator that handles transcript fetch, analysis,
//! clip task generation, and style routing using the new processor framework.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

use vclip_firestore::{types::ToFirestoreValue, FirestoreClient};
use vclip_media::{
    core::{MetricsCollector, SecurityContext, StyleProcessorRegistry},
    download_video,
    styles::StyleProcessorFactory as MediaStyleProcessorFactory,
};
use vclip_models::VideoMetadata;
use vclip_queue::{ProcessVideoJob, ProgressChannel, RenderSceneStyleJob, ReprocessScenesJob};
use vclip_storage::R2Client;

use crate::clip_pipeline::{self, ClipProcessingResults};
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

    // Source video lifecycle coordination
    pub source_coordinator: crate::source_video_coordinator::SourceVideoCoordinator,
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

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let progress = ProgressChannel::new(&redis_url).map_err(|e| WorkerError::Queue(e))?;

        let ffmpeg_semaphore = Arc::new(Semaphore::new(config.max_ffmpeg_processes));

        // Initialize new architecture components
        let metrics = Arc::new(MetricsCollector::new());
        let security = Arc::new(SecurityContext::new());
        let style_factory = MediaStyleProcessorFactory::new();
        let mut style_registry = StyleProcessorRegistry::new();
        style_registry.register_factory(Arc::new(style_factory));

        // Initialize source video coordinator for distributed cleanup
        let source_coordinator = crate::source_video_coordinator::SourceVideoCoordinator::new(&redis_url)
            .map_err(|e| WorkerError::Queue(vclip_queue::QueueError::Redis(e)))?;

        Ok(Self {
            config,
            storage,
            firestore,
            progress,
            ffmpeg_semaphore,
            style_registry: Arc::new(style_registry),
            metrics,
            security,
            source_coordinator,
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
        let analysis_data = self
            .download_and_analyze(ctx, job, &work_dir, &transcript_data)
            .await?;
        ctx.progress.progress(&job.job_id, 35).await.ok();

        // Store video metadata
        self.store_video_metadata(ctx, job, &transcript_data, &analysis_data)
            .await?;

        // Generate and process clips
        let clip_results = self
            .process_clips(ctx, job, &work_dir, &analysis_data)
            .await?;

        // Finalize video
        self.finalize_video(ctx, job, clip_results.total_processed as u32)
            .await?;

        job_logger.log_completion(&format!(
            "Processed {} clips successfully",
            clip_results.completed_count
        ));

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

        let base_prompt = job
            .custom_prompt
            .as_deref()
            .map(|s| s.to_string())
            .or_else(|| load_prompt_from_file())
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

        let (real_video_title, canonical_video_url) = self
            .gemini_client
            .get_video_metadata(&job.video_url)
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to get video metadata: {}", e)))?;

        let transcript = self
            .gemini_client
            .get_transcript_only(
                &job.video_url,
                &PathBuf::from(&ctx.config.work_dir).join("temp"),
            )
            .await
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
            self.gemini_client.analyze_transcript(
                &transcript.prompt,
                &job.video_url,
                &transcript.content
            )
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
            highlights: analysis
                .highlights
                .highlights
                .iter()
                .map(|h| vclip_storage::operations::HighlightEntry {
                    id: h.id,
                    title: h.title.clone(),
                    description: h.description.clone(),
                    start: h.start.clone(),
                    end: h.end.clone(),
                    duration: h.duration,
                    pad_before_seconds: h.pad_before_seconds,
                    pad_after_seconds: h.pad_after_seconds,
                    hook_category: h.hook_category.clone(),
                    reason: h.reason.clone(),
                })
                .collect(),
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
            fields.insert(
                "video_title".to_string(),
                transcript.title.clone().to_firestore_value(),
            );
            fields.insert(
                "video_url".to_string(),
                transcript.url.clone().to_firestore_value(),
            );
            fields.insert(
                "status".to_string(),
                vclip_models::VideoStatus::Processing
                    .as_str()
                    .to_firestore_value(),
            );

            ctx.firestore
                .update_document(
                    &format!("users/{}/videos", job.user_id),
                    job.video_id.as_str(),
                    fields,
                    Some(vec![
                        "video_title".to_string(),
                        "video_url".to_string(),
                        "status".to_string(),
                    ]),
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
        clip_pipeline::process_clips(ctx, job, work_dir, analysis).await
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
                info!(
                    "Successfully marked video {} as completed with {} clips",
                    job.video_id, completed_clips
                );
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

        info!(
            "Completed video job: {} ({} clips)",
            job.job_id, completed_clips
        );
        Ok(())
    }

    /// Process a reprocess scenes job.
    /// Delegates to the reprocessing module for the full implementation.
    pub async fn reprocess_scenes_job(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ReprocessScenesJob,
    ) -> WorkerResult<()> {
        crate::reprocessing::reprocess_scenes(ctx, job).await
    }

    /// Process a single render job (fine-grained: one scene, one style).
    ///
    /// Delegates to the render_job module which handles coordinated source video
    /// lifecycle across distributed workers.
    pub async fn process_render_job(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &RenderSceneStyleJob,
    ) -> WorkerResult<()> {
        crate::render_job::process_render_job(ctx, job).await
    }
}

/// Data structures for processing pipeline.
struct TranscriptData {
    title: String,
    url: String,
    content: String,
    prompt: String,
}

pub struct AnalysisData {
    pub video_file: PathBuf,
    pub highlights: crate::gemini::HighlightsResponse,
}

// Re-export JobLogger from logging module for backward compatibility
pub use crate::logging::JobLogger;

