//! Job types for the queue.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vclip_models::{AspectRatio, CropMode, JobId, Style, VideoId};

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

/// Generic job wrapper for queue storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueJob {
    /// Analysis job: download transcript, analyze, create draft with scenes
    AnalyzeVideo(AnalyzeVideoJob),
    /// Orchestration job: analyze video and fan out render jobs (legacy, being phased out)
    ProcessVideo(ProcessVideoJob),
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
            QueueJob::ReprocessScenes(j) => &j.job_id,
            QueueJob::RenderSceneStyle(j) => &j.job_id,
        }
    }

    pub fn user_id(&self) -> &str {
        match self {
            QueueJob::AnalyzeVideo(j) => &j.user_id,
            QueueJob::ProcessVideo(j) => &j.user_id,
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

