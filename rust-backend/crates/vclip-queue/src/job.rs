//! Job types for the queue.

use serde::{Deserialize, Serialize};
use vclip_models::{AspectRatio, CropMode, JobId, Style, VideoId};

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

    /// Generate idempotency key for deduplication.
    pub fn idempotency_key(&self) -> String {
        format!("reprocess:{}:{}", self.user_id, self.video_id)
    }
}

/// Generic job wrapper for queue storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueJob {
    ProcessVideo(ProcessVideoJob),
    ReprocessScenes(ReprocessScenesJob),
}

impl QueueJob {
    pub fn job_id(&self) -> &JobId {
        match self {
            QueueJob::ProcessVideo(j) => &j.job_id,
            QueueJob::ReprocessScenes(j) => &j.job_id,
        }
    }

    pub fn user_id(&self) -> &str {
        match self {
            QueueJob::ProcessVideo(j) => &j.user_id,
            QueueJob::ReprocessScenes(j) => &j.user_id,
        }
    }

    pub fn video_id(&self) -> &VideoId {
        match self {
            QueueJob::ProcessVideo(j) => &j.video_id,
            QueueJob::ReprocessScenes(j) => &j.video_id,
        }
    }

    pub fn idempotency_key(&self) -> String {
        match self {
            QueueJob::ProcessVideo(j) => j.idempotency_key(),
            QueueJob::ReprocessScenes(j) => j.idempotency_key(),
        }
    }
}
