//! Video processing orchestration.
//!
//! Modular, testable coordinator that handles transcript fetch, analysis,
//! clip task generation, and style routing using the new processor framework.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

use vclip_firestore::{types::ToFirestoreValue, AnalysisDraftRepository, FirestoreClient};
use vclip_media::{
    core::{MetricsCollector, SecurityContext, StyleProcessorRegistry},
    styles::StyleProcessorFactory as MediaStyleProcessorFactory,
};
use vclip_models::{AnalysisStatus, DraftScene, VideoMetadata};
use vclip_queue::{AnalyzeVideoJob, DownloadSourceJob, ProcessVideoJob, ProgressChannel, RenderSceneStyleJob, ReprocessScenesJob};
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

    // Source video lifecycle coordination
    pub source_coordinator: crate::source_video_coordinator::SourceVideoCoordinator,

    // Neural analysis cache service
    pub neural_cache: crate::neural_cache::NeuralCacheService,

    // Raw segment cache service (Phase 4)
    pub raw_cache: crate::raw_segment_cache::RawSegmentCacheService,

    // Redis client for single-flight locks
    pub redis: redis::Client,

    // Shared job queue client (avoids repeated from_env() calls)
    pub job_queue: Option<vclip_queue::JobQueue>,
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
        let source_coordinator =
            crate::source_video_coordinator::SourceVideoCoordinator::new(&redis_url)
                .map_err(|e| WorkerError::Queue(vclip_queue::QueueError::Redis(e)))?;

        // Initialize Redis client for single-flight locks
        let redis = redis::Client::open(redis_url.as_str())
            .map_err(|e| WorkerError::Queue(vclip_queue::QueueError::Redis(e)))?;

        // Initialize neural cache service
        let neural_cache =
            crate::neural_cache::NeuralCacheService::new(storage.clone(), redis.clone());

        // Initialize raw segment cache service (Phase 4)
        let raw_cache =
            crate::raw_segment_cache::RawSegmentCacheService::new(storage.clone(), redis.clone());

        // Initialize shared job queue client (centralized, avoids hot-path from_env() calls)
        let job_queue = match vclip_queue::JobQueue::from_env() {
            Ok(q) => Some(q),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to create shared job queue client (background jobs will be skipped)"
                );
                None
            }
        };

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
            neural_cache,
            raw_cache,
            redis,
            job_queue,
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
    /// 
    /// This now only performs analysis (transcript + AI scene detection).
    /// Video download and clip processing happen later via ReprocessScenes or RenderSceneStyle jobs.
    pub async fn process_video_job(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
    ) -> WorkerResult<()> {
        let job_logger = JobLogger::new(&job.job_id, "video_processing");

        job_logger.log_start("Starting video analysis job");

        // Create work directory
        let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
        tokio::fs::create_dir_all(&work_dir).await?;

        ctx.progress
            .log(&job.job_id, "Starting video analysis...")
            .await
            .ok();
        ctx.progress.progress(&job.job_id, 5).await.ok();

        // Get transcript and video metadata (NO video download)
        let transcript_data = self.fetch_transcript_and_metadata(ctx, job).await?;
        ctx.progress.progress(&job.job_id, 20).await.ok();

        // Analyze transcript with AI to get scenes (NO video download)
        ctx.progress
            .log(&job.job_id, "Analyzing content with AI...")
            .await
            .ok();
        
        let plan_path = work_dir.join("clip_plan.json");
        
        // Attempt to reuse a previously persisted plan to avoid non-determinism on retries.
        let cached_plan = tokio::fs::read(&plan_path).await.ok().and_then(|bytes| {
            serde_json::from_slice::<crate::gemini::HighlightsResponse>(&bytes).ok()
        });

        let ai_response = if let Some(plan) = cached_plan {
            plan
        } else {
            let analysis_result = self
                .gemini_client
                .analyze_transcript(
                    &transcript_data.prompt,
                    &job.video_url,
                    &transcript_data.content,
                )
                .await?;
            
            // Persist the plan for deterministic retries.
            match serde_json::to_vec_pretty(&analysis_result) {
                Ok(bytes) => {
                    if let Err(e) = tokio::fs::write(&plan_path, bytes).await {
                        tracing::warn!(
                            path = ?plan_path,
                            error = %e,
                            "Failed to persist clip plan for retry determinism"
                        );
                    }
                }
                Err(e) => tracing::warn!("Failed to serialize clip plan: {}", e),
            }
            
            analysis_result
        };

        ctx.progress.progress(&job.job_id, 70).await.ok();

        let scene_count = ai_response.highlights.len();
        ctx.progress
            .log(&job.job_id, &format!("Found {} scenes ready for processing", scene_count))
            .await
            .ok();

        // Store video metadata and highlights (NO video file path needed)
        self.store_video_metadata_analysis_only(ctx, job, &transcript_data, &ai_response)
            .await?;

        ctx.progress.progress(&job.job_id, 90).await.ok();

        // Mark video as ready for scene selection (not fully completed)
        let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
        video_repo
            .update_status(&job.video_id, vclip_models::VideoStatus::Analyzed)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        // Enqueue background source download using shared queue client
        if let Some(ref queue) = ctx.job_queue {
            let download_job = DownloadSourceJob {
                job_id: vclip_models::JobId::new(),
                user_id: job.user_id.clone(),
                video_id: job.video_id.clone(),
                video_url: transcript_data.url.clone(),
                created_at: chrono::Utc::now(),
            };
            let download_video_id = download_job.video_id.clone();

            // Enqueue synchronously (non-blocking Redis call, fast enough for hot path)
            match queue.enqueue_download_source(download_job).await {
                Ok(message_id) => {
                    tracing::info!(
                        video_id = %download_video_id,
                        message_id = %message_id,
                        "Enqueued DownloadSourceJob after analysis"
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        video_id = %download_video_id,
                        error = %e,
                        "Failed to enqueue DownloadSourceJob after analysis (non-critical)"
                    );
                }
            }
        }

        ctx.progress.progress(&job.job_id, 100).await.ok();
        ctx.progress
            .done(&job.job_id, job.video_id.as_str())
            .await
            .ok();

        job_logger.log_completion(&format!(
            "Analysis complete: {} scenes found",
            scene_count
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
            .log(&job.job_id, "Fetching video information...")
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

    /// Store video metadata and highlights for analysis-only flow (no video file).
    async fn store_video_metadata_analysis_only(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &ProcessVideoJob,
        transcript: &TranscriptData,
        highlights: &crate::gemini::HighlightsResponse,
    ) -> WorkerResult<()> {
        let video_meta = VideoMetadata::new(
            job.video_id.clone(),
            &job.user_id,
            &transcript.url,
            &transcript.title,
        );

        let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

        // Convert to VideoHighlights for Firestore
        let video_highlights = vclip_models::highlight::VideoHighlights {
            video_id: job.video_id.as_str().to_string(),
            highlights: highlights
                .highlights
                .iter()
                .map(|h| {
                    let mut highlight = vclip_models::Highlight::new(
                        h.id,
                        h.title.clone(),
                        h.start.clone(),
                        h.end.clone(),
                    )
                    .with_calculated_duration();
                    if highlight.duration == 0 {
                        highlight.duration = h.duration;
                    }
                    highlight.pad_before = h.pad_before_seconds;
                    highlight.pad_after = h.pad_after_seconds;
                    highlight.hook_category = h.hook_category.as_ref().and_then(|cat| {
                        match cat.to_lowercase().as_str() {
                            "emotional" => Some(vclip_models::HighlightCategory::Emotional),
                            "educational" => Some(vclip_models::HighlightCategory::Educational),
                            "controversial" => Some(vclip_models::HighlightCategory::Controversial),
                            "inspirational" => Some(vclip_models::HighlightCategory::Inspirational),
                            "humorous" | "funny" => Some(vclip_models::HighlightCategory::Humorous),
                            "dramatic" => Some(vclip_models::HighlightCategory::Dramatic),
                            "surprising" => Some(vclip_models::HighlightCategory::Surprising),
                            _ => Some(vclip_models::HighlightCategory::Other),
                        }
                    });
                    highlight.reason = h.reason.clone();
                    highlight.description = h.description.clone();
                    highlight
                })
                .collect(),
            video_url: Some(transcript.url.clone()),
            video_title: Some(transcript.title.clone()),
            custom_prompt: job.custom_prompt.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store highlights in Firestore (source of truth)
        let highlights_repo = vclip_firestore::HighlightsRepository::new(
            ctx.firestore.clone(),
            &job.user_id,
        );
        
        highlights_repo
            .upsert(&video_highlights)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        // Create video record (or update if exists)
        if let Ok(Some(_existing_video)) = video_repo.get(&job.video_id).await {
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
                "highlights_count".to_string(),
                (highlights.highlights.len() as u32).to_firestore_value(),
            );

            ctx.firestore
                .update_document(
                    &format!("users/{}/videos", job.user_id),
                    job.video_id.as_str(),
                    fields,
                    Some(vec![
                        "video_title".to_string(),
                        "video_url".to_string(),
                        "highlights_count".to_string(),
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

        info!(
            "Stored analysis results for video {}: {} highlights",
            job.video_id,
            highlights.highlights.len()
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

    /// Process an analyze video job (new two-step workflow).
    ///
    /// Downloads transcript, analyzes with AI, and stores results as an AnalysisDraft
    /// with DraftScenes in Firestore. Does NOT render any clips.
    pub async fn process_analyze_job(
        &self,
        ctx: &EnhancedProcessingContext,
        job: &AnalyzeVideoJob,
    ) -> WorkerResult<()> {
        let job_logger = JobLogger::new(&job.job_id, "analyze_video");
        job_logger.log_start("Starting video analysis job");

        let draft_repo = AnalysisDraftRepository::new(ctx.firestore.clone(), &job.user_id);

        // Update status to Downloading
        draft_repo
            .update_status(&job.draft_id, AnalysisStatus::Downloading, None)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        ctx.progress
            .log(&job.job_id, "Fetching video details...")
            .await
            .ok();
        ctx.progress.progress(&job.job_id, 10).await.ok();

        // Get video metadata and transcript
        let (video_title, _canonical_url) = self
            .gemini_client
            .get_video_metadata(&job.video_url)
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to get video metadata: {}", e)))?;

        let work_dir = std::path::PathBuf::from(&ctx.config.work_dir).join(&job.draft_id);
        tokio::fs::create_dir_all(&work_dir).await?;

        let transcript = self
            .gemini_client
            .get_transcript_only(&job.video_url, &work_dir)
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to get transcript: {}", e)))?;

        ctx.progress.progress(&job.job_id, 30).await.ok();

        // Update status to Analyzing
        draft_repo
            .update_status(&job.draft_id, AnalysisStatus::Analyzing, None)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        ctx.progress
            .log(&job.job_id, "Analyzing content with AI...")
            .await
            .ok();

        // Build prompt
        let base_prompt = job
            .prompt_instructions
            .clone()
            .or_else(load_prompt_from_file)
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

        // Analyze transcript
        let analysis = self
            .gemini_client
            .analyze_transcript(&base_prompt, &job.video_url, &transcript)
            .await?;

        ctx.progress.progress(&job.job_id, 80).await.ok();

        // Convert highlights to DraftScenes
        let scenes: Vec<DraftScene> = analysis
            .highlights
            .iter()
            .map(|h| {
                let computed = vclip_models::Highlight::new(
                    h.id,
                    "temp",
                    h.start.clone(),
                    h.end.clone(),
                )
                .with_calculated_duration();
                DraftScene {
                    id: h.id,
                    analysis_draft_id: job.draft_id.clone(),
                    title: h.title.clone(),
                    description: h.description.clone(),
                    reason: h.reason.clone(),
                    start: h.start.clone(),
                    end: h.end.clone(),
                    duration_secs: if computed.duration == 0 {
                        h.duration
                    } else {
                        computed.duration
                    },
                    pad_before: h.pad_before_seconds,
                    pad_after: h.pad_after_seconds,
                    confidence: None, // Confidence scoring not yet implemented in AI response
                    hook_category: h.hook_category.clone(),
                }
            })
            .collect();

        let scene_count = scenes.len() as u32;

        // Store scenes in Firestore
        draft_repo
            .upsert_scenes(&job.draft_id, &scenes)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        // Update draft with completion info
        draft_repo
            .update_completion(&job.draft_id, Some(video_title.clone()), scene_count, 0)
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        ctx.progress.progress(&job.job_id, 100).await.ok();
        ctx.progress.done(&job.job_id, &job.draft_id).await.ok();

        // Cleanup work directory
        if let Err(e) = tokio::fs::remove_dir_all(&work_dir).await {
            tracing::warn!("Failed to cleanup work directory {:?}: {}", work_dir, e);
        }

        job_logger.log_completion(&format!(
            "Analyzed video '{}' with {} scenes",
            video_title, scene_count
        ));

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

pub struct AnalysisData {
    pub video_file: PathBuf,
    pub highlights: crate::gemini::HighlightsResponse,
}

// Re-export JobLogger from logging module for backward compatibility
pub use crate::logging::JobLogger;
