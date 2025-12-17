//! Job types for the queue.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vclip_models::{AspectRatio, CropMode, DetectionTier, JobId, StreamerSplitParams, Style, VideoId};

fn default_neural_detection_tier() -> DetectionTier {
    // Backward compatibility: previously we always computed the highest tier.
    DetectionTier::SpeakerAware
}

/// Job to analyze a video and create a draft with scenes.
///
/// This is the first step in the two-step workflow. The job downloads
/// the transcript, analyzes it for highlights, and stores the results
/// as an AnalysisDraft with DraftScenes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeVideoJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Pre-generated draft ID
    pub draft_id: String,
    /// Video URL to analyze
    pub video_url: String,
    /// Optional AI instructions from user
    pub prompt_instructions: Option<String>,
    /// When the job was created
    pub created_at: DateTime<Utc>,
}

impl AnalyzeVideoJob {
    /// Create a new analyze job.
    pub fn new(
        user_id: impl Into<String>,
        draft_id: impl Into<String>,
        video_url: impl Into<String>,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            user_id: user_id.into(),
            draft_id: draft_id.into(),
            video_url: video_url.into(),
            prompt_instructions: None,
            created_at: Utc::now(),
        }
    }

    /// Set prompt instructions.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt_instructions = Some(prompt.into());
        self
    }

    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        format!("analyze:{}:{}", self.user_id, self.draft_id)
    }
}

/// Job to process a new video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessVideoJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Video ID
    pub video_id: VideoId,
    /// Video URL
    pub video_url: String,
    /// Styles to process
    pub styles: Vec<Style>,
    /// Crop mode
    pub crop_mode: CropMode,
    /// Target aspect ratio
    pub target_aspect: AspectRatio,
    /// Custom prompt for AI analysis
    pub custom_prompt: Option<String>,
}

impl ProcessVideoJob {
    pub fn new(
        user_id: impl Into<String>,
        video_url: impl Into<String>,
        styles: Vec<Style>,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            user_id: user_id.into(),
            video_id: VideoId::new(),
            video_url: video_url.into(),
            styles,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            custom_prompt: None,
        }
    }

    /// Set crop mode.
    pub fn with_crop_mode(mut self, crop_mode: CropMode) -> Self {
        self.crop_mode = crop_mode;
        self
    }

    /// Set target aspect ratio.
    pub fn with_target_aspect(mut self, aspect: AspectRatio) -> Self {
        self.target_aspect = aspect;
        self
    }

    /// Set custom prompt.
    pub fn with_custom_prompt(mut self, prompt: Option<String>) -> Self {
        self.custom_prompt = prompt;
        self
    }

    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        format!("process:{}:{}", self.user_id, self.video_id)
    }
}

/// Job to reprocess specific scenes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReprocessScenesJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Video ID
    pub video_id: VideoId,
    /// Scene IDs to reprocess
    pub scene_ids: Vec<u32>,
    /// Styles to apply
    pub styles: Vec<Style>,
    /// Crop mode
    pub crop_mode: CropMode,
    /// Target aspect ratio
    pub target_aspect: AspectRatio,
    /// Enable object detection for Cinematic tier (default: false)
    #[serde(default)]
    pub enable_object_detection: bool,
    /// When true, overwrite existing clips instead of skipping them
    #[serde(default)]
    pub overwrite: bool,
    /// Optional StreamerSplit parameters for user-controlled crop position/zoom
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamer_split_params: Option<StreamerSplitParams>,
    /// Enable Top Scenes compilation mode (creates single video from all scenes with countdown overlay)
    #[serde(default)]
    pub top_scenes_compilation: bool,
    /// Cut silent parts from clips using VAD (default: true for more dynamic content)
    #[serde(default = "default_cut_silent_parts")]
    pub cut_silent_parts: bool,
}

fn default_cut_silent_parts() -> bool {
    false
}

impl ReprocessScenesJob {
    pub fn new(
        user_id: impl Into<String>,
        video_id: VideoId,
        scene_ids: Vec<u32>,
        styles: Vec<Style>,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            user_id: user_id.into(),
            video_id,
            scene_ids,
            styles,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            enable_object_detection: false,
            overwrite: false,
            streamer_split_params: None,
            top_scenes_compilation: false,
            cut_silent_parts: false,
        }
    }

    /// Set cut silent parts option.
    pub fn with_cut_silent_parts(mut self, enabled: bool) -> Self {
        self.cut_silent_parts = enabled;
        self
    }

    /// Check if this job is a Top Scenes compilation.
    pub fn is_top_scenes_compilation(&self) -> bool {
        self.top_scenes_compilation && self.styles.contains(&Style::StreamerTopScenes)
    }

    /// Set top scenes compilation mode.
    pub fn with_top_scenes_compilation(mut self, enabled: bool) -> Self {
        self.top_scenes_compilation = enabled;
        self
    }

    /// Set crop mode.
    pub fn with_crop_mode(mut self, crop_mode: CropMode) -> Self {
        self.crop_mode = crop_mode;
        self
    }

    /// Set target aspect ratio.
    pub fn with_target_aspect(mut self, aspect: AspectRatio) -> Self {
        self.target_aspect = aspect;
        self
    }

    /// Set overwrite mode (re-render existing clips).
    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Set StreamerSplit parameters for user-controlled crop position/zoom.
    pub fn with_streamer_split_params(mut self, params: Option<StreamerSplitParams>) -> Self {
        self.streamer_split_params = params;
        self
    }

    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        // Sort scene_ids and styles for consistent ordering
        let mut scene_ids = self.scene_ids.clone();
        scene_ids.sort();
        let mut styles: Vec<String> = self.styles.iter().map(|s| s.to_string()).collect();
        styles.sort();
        
        format!(
            "reprocess:{}:{}:{:?}:{:?}:{}:{}",
            self.user_id,
            self.video_id,
            scene_ids,
            styles,
            self.crop_mode,
            self.target_aspect
        )
    }
}

/// Job to render a single clip (one scene with one style).
///
/// This is the atomic unit of work for parallel processing. Each job
/// produces exactly one clip, enabling fine-grained parallelization
/// and better failure isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSceneStyleJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Video ID
    pub video_id: VideoId,
    /// Scene/highlight ID
    pub scene_id: u32,
    /// Scene title (for logging/progress)
    pub scene_title: String,
    /// Single style to render
    pub style: Style,
    /// Crop mode
    pub crop_mode: CropMode,
    /// Target aspect ratio
    pub target_aspect: AspectRatio,
    /// Highlight start timestamp (format: "MM:SS" or "HH:MM:SS")
    pub start: String,
    /// Highlight end timestamp
    pub end: String,
    /// Optional padding before highlight start
    pub pad_before_seconds: Option<f64>,
    /// Optional padding after highlight end
    pub pad_after_seconds: Option<f64>,
    /// Parent job ID for tracking (orchestration job that created this)
    pub parent_job_id: Option<JobId>,
    /// Enable object detection for Cinematic tier (default: false)
    #[serde(default)]
    pub enable_object_detection: bool,
}

impl RenderSceneStyleJob {
    /// Create a new render job.
    pub fn new(
        user_id: impl Into<String>,
        video_id: VideoId,
        scene_id: u32,
        scene_title: impl Into<String>,
        style: Style,
        start: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            user_id: user_id.into(),
            video_id,
            scene_id,
            scene_title: scene_title.into(),
            style,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            start: start.into(),
            end: end.into(),
            pad_before_seconds: None,
            pad_after_seconds: None,
            parent_job_id: None,
            enable_object_detection: false,
        }
    }

    /// Set crop mode.
    pub fn with_crop_mode(mut self, crop_mode: CropMode) -> Self {
        self.crop_mode = crop_mode;
        self
    }

    /// Set target aspect ratio.
    pub fn with_target_aspect(mut self, aspect: AspectRatio) -> Self {
        self.target_aspect = aspect;
        self
    }

    /// Set padding before.
    pub fn with_pad_before(mut self, seconds: Option<f64>) -> Self {
        self.pad_before_seconds = seconds;
        self
    }

    /// Set padding after.
    pub fn with_pad_after(mut self, seconds: Option<f64>) -> Self {
        self.pad_after_seconds = seconds;
        self
    }

    /// Set parent job ID.
    pub fn with_parent_job(mut self, parent_id: JobId) -> Self {
        self.parent_job_id = Some(parent_id);
        self
    }

    /// Generate idempotency key for deduplication.
    ///
    /// The key uniquely identifies a specific (video, scene, style, settings)
    /// combination to prevent duplicate processing.
    pub fn idempotency_key(&self) -> String {
        format!(
            "render:{}:{}:{}:{}:{}:{}",
            self.user_id,
            self.video_id,
            self.scene_id,
            self.style,
            self.crop_mode,
            self.target_aspect
        )
    }
}

/// Job to download the source video in the background.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSourceJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Video ID
    pub video_id: VideoId,
    /// Video URL
    pub video_url: String,
    /// When the job was created
    pub created_at: DateTime<Utc>,
}

impl DownloadSourceJob {
    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        format!("download_source:{}:{}", self.user_id, self.video_id)
    }
}

/// Job to compute and cache neural analysis for a scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralAnalysisJob {
    /// Unique job ID
    pub job_id: JobId,
    /// User ID
    pub user_id: String,
    /// Video ID
    pub video_id: VideoId,
    /// Scene ID to analyze
    pub scene_id: u32,
    /// Optional R2 key hint for source video
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hint_r2_key: Option<String>,

    /// Detection tier required for this analysis.
    ///
    /// Defaults to SpeakerAware for backward compatibility with older serialized jobs.
    #[serde(default = "default_neural_detection_tier")]
    pub detection_tier: DetectionTier,
    /// When the job was created
    pub created_at: DateTime<Utc>,
}

impl NeuralAnalysisJob {
    /// Create a new neural analysis job.
    pub fn new(
        user_id: impl Into<String>,
        video_id: VideoId,
        scene_id: u32,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            user_id: user_id.into(),
            video_id,
            scene_id,
            source_hint_r2_key: None,
            detection_tier: default_neural_detection_tier(),
            created_at: Utc::now(),
        }
    }

    /// Set detection tier.
    pub fn with_detection_tier(mut self, tier: DetectionTier) -> Self {
        self.detection_tier = tier;
        self
    }

    /// Set source video R2 key hint.
    pub fn with_source_hint(mut self, key: impl Into<String>) -> Self {
        self.source_hint_r2_key = Some(key.into());
        self
    }

    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        format!(
            "neural:{}:{}:{}:{}",
            self.user_id, self.video_id, self.scene_id, self.detection_tier
        )
    }
}

/// Generic job wrapper for queue storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueJob {
    /// Analysis job: download transcript, analyze, create draft with scenes
    AnalyzeVideo(AnalyzeVideoJob),
    /// Orchestration job: analyze video and fan out render jobs (legacy, being phased out)
    ProcessVideo(ProcessVideoJob),
    /// Background job: download source video
    DownloadSource(DownloadSourceJob),
    /// Background job: compute neural analysis for a scene
    NeuralAnalysis(NeuralAnalysisJob),
    /// Orchestration job: load highlights and fan out render jobs for selected scenes
    ReprocessScenes(ReprocessScenesJob),
    /// Fine-grained job: render a single (scene, style) clip
    RenderSceneStyle(RenderSceneStyleJob),
}

impl QueueJob {
    pub fn job_id(&self) -> &JobId {
        match self {
            QueueJob::AnalyzeVideo(j) => &j.job_id,
            QueueJob::ProcessVideo(j) => &j.job_id,
            QueueJob::DownloadSource(j) => &j.job_id,
            QueueJob::NeuralAnalysis(j) => &j.job_id,
            QueueJob::ReprocessScenes(j) => &j.job_id,
            QueueJob::RenderSceneStyle(j) => &j.job_id,
        }
    }

    pub fn user_id(&self) -> &str {
        match self {
            QueueJob::AnalyzeVideo(j) => &j.user_id,
            QueueJob::ProcessVideo(j) => &j.user_id,
            QueueJob::DownloadSource(j) => &j.user_id,
            QueueJob::NeuralAnalysis(j) => &j.user_id,
            QueueJob::ReprocessScenes(j) => &j.user_id,
            QueueJob::RenderSceneStyle(j) => &j.user_id,
        }
    }

    /// Returns the video_id if applicable.
    /// AnalyzeVideo doesn't have a video_id yet (draft_id instead).
    pub fn video_id(&self) -> Option<&VideoId> {
        match self {
            QueueJob::AnalyzeVideo(_) => None,
            QueueJob::ProcessVideo(j) => Some(&j.video_id),
            QueueJob::DownloadSource(j) => Some(&j.video_id),
            QueueJob::NeuralAnalysis(j) => Some(&j.video_id),
            QueueJob::ReprocessScenes(j) => Some(&j.video_id),
            QueueJob::RenderSceneStyle(j) => Some(&j.video_id),
        }
    }

    /// Returns the draft_id if this is an AnalyzeVideo job.
    pub fn draft_id(&self) -> Option<&str> {
        match self {
            QueueJob::AnalyzeVideo(j) => Some(&j.draft_id),
            _ => None,
        }
    }

    pub fn idempotency_key(&self) -> String {
        match self {
            QueueJob::AnalyzeVideo(j) => j.idempotency_key(),
            QueueJob::ProcessVideo(j) => j.idempotency_key(),
            QueueJob::DownloadSource(j) => j.idempotency_key(),
            QueueJob::NeuralAnalysis(j) => j.idempotency_key(),
            QueueJob::ReprocessScenes(j) => j.idempotency_key(),
            QueueJob::RenderSceneStyle(j) => j.idempotency_key(),
        }
    }

    /// Returns true if this is an analysis job.
    pub fn is_analysis(&self) -> bool {
        matches!(self, QueueJob::AnalyzeVideo(_))
    }

    /// Returns true if this is an orchestration job (ProcessVideo or ReprocessScenes).
    pub fn is_orchestration(&self) -> bool {
        matches!(self, QueueJob::ProcessVideo(_) | QueueJob::ReprocessScenes(_))
    }

    /// Returns true if this is a fine-grained render job.
    pub fn is_render(&self) -> bool {
        matches!(self, QueueJob::RenderSceneStyle(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_job_download_source_serde_roundtrip() {
        let job = DownloadSourceJob {
            job_id: JobId::new(),
            user_id: "user_1".to_string(),
            video_id: VideoId::new(),
            video_url: "https://example.com/video".to_string(),
            created_at: Utc::now(),
        };

        let wrapper = QueueJob::DownloadSource(job.clone());
        let json = serde_json::to_string(&wrapper).expect("serialize QueueJob");
        let decoded: QueueJob = serde_json::from_str(&json).expect("deserialize QueueJob");

        match decoded {
            QueueJob::DownloadSource(j) => {
                assert_eq!(j.job_id, job.job_id);
                assert_eq!(j.user_id, job.user_id);
                assert_eq!(j.video_id, job.video_id);
                assert_eq!(j.video_url, job.video_url);
                assert_eq!(j.created_at, job.created_at);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
