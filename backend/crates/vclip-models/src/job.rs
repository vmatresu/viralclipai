//! Job definitions for queue processing.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::{AspectRatio, ClipTask, CropMode, Style, VideoId};

/// Unique identifier for a job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct JobId(pub String);

impl JobId {
    /// Generate a new random job ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Job state in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Job is waiting in queue
    #[default]
    Pending,
    /// Job is being processed
    Processing,
    /// Job completed successfully
    Completed,
    /// Job failed (may be retried)
    Failed,
    /// Job sent to DLQ after max retries
    DeadLettered,
}

impl JobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Processing => "processing",
            JobState::Completed => "completed",
            JobState::Failed => "failed",
            JobState::DeadLettered => "dead_lettered",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, JobState::Completed | JobState::DeadLettered)
    }
}

/// Type of job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Process a new video
    ProcessVideo,
    /// Reprocess specific scenes
    ReprocessScenes,
}

/// A job to be processed by the worker.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Job {
    /// Unique job ID
    pub id: JobId,

    /// Job type
    pub job_type: JobType,

    /// User ID
    pub user_id: String,

    /// Video ID
    pub video_id: VideoId,

    /// Video URL (for ProcessVideo jobs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,

    /// Styles to process
    pub styles: Vec<Style>,

    /// Crop mode
    #[serde(default)]
    pub crop_mode: CropMode,

    /// Target aspect ratio
    #[serde(default)]
    pub target_aspect: AspectRatio,

    /// Custom prompt for AI analysis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,

    /// Scene IDs to reprocess (for ReprocessScenes jobs)
    #[serde(default)]
    pub scene_ids: Vec<u32>,

    /// Pre-computed clip tasks (optional, for optimization)
    #[serde(default)]
    pub clip_tasks: Vec<ClipTask>,

    /// Job state
    #[serde(default)]
    pub state: JobState,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Started at timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// Completed at timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Number of retry attempts
    #[serde(default)]
    pub retry_count: u32,

    /// Maximum retries allowed
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Progress (0-100)
    #[serde(default)]
    pub progress: u8,

    /// Idempotency key (user_id:video_id for dedup)
    pub idempotency_key: String,
}

fn default_max_retries() -> u32 {
    3
}

impl Job {
    /// Create a new ProcessVideo job.
    pub fn new_process_video(
        user_id: impl Into<String>,
        video_url: impl Into<String>,
        styles: Vec<Style>,
    ) -> Self {
        let user_id = user_id.into();
        let video_id = VideoId::new();
        let now = Utc::now();

        Self {
            id: JobId::new(),
            job_type: JobType::ProcessVideo,
            user_id: user_id.clone(),
            video_id: video_id.clone(),
            video_url: Some(video_url.into()),
            styles,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            custom_prompt: None,
            scene_ids: Vec::new(),
            clip_tasks: Vec::new(),
            state: JobState::Pending,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: default_max_retries(),
            error_message: None,
            progress: 0,
            idempotency_key: format!("{}:{}", user_id, video_id),
        }
    }

    /// Create a new ReprocessScenes job.
    pub fn new_reprocess_scenes(
        user_id: impl Into<String>,
        video_id: VideoId,
        scene_ids: Vec<u32>,
        styles: Vec<Style>,
    ) -> Self {
        let user_id = user_id.into();
        let now = Utc::now();

        Self {
            id: JobId::new(),
            job_type: JobType::ReprocessScenes,
            user_id: user_id.clone(),
            video_id: video_id.clone(),
            video_url: None,
            styles,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            custom_prompt: None,
            scene_ids,
            clip_tasks: Vec::new(),
            state: JobState::Pending,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: default_max_retries(),
            error_message: None,
            progress: 0,
            idempotency_key: format!("reprocess:{}:{}", user_id, video_id),
        }
    }

    /// Start processing the job.
    pub fn start(mut self) -> Self {
        self.state = JobState::Processing;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self
    }

    /// Mark job as completed.
    pub fn complete(mut self) -> Self {
        self.state = JobState::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.progress = 100;
        self
    }

    /// Mark job as failed.
    pub fn fail(mut self, error: impl Into<String>) -> Self {
        self.state = JobState::Failed;
        self.error_message = Some(error.into());
        self.updated_at = Utc::now();
        self.retry_count += 1;
        self
    }

    /// Send to dead letter queue.
    pub fn dead_letter(mut self) -> Self {
        self.state = JobState::DeadLettered;
        self.updated_at = Utc::now();
        self
    }

    /// Check if job can be retried.
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries && self.state == JobState::Failed
    }

    /// Update progress.
    pub fn with_progress(mut self, progress: u8) -> Self {
        self.progress = progress.min(100);
        self.updated_at = Utc::now();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let job = Job::new_process_video(
            "user123",
            "https://youtube.com/watch?v=abc",
            vec![Style::Split],
        );

        assert_eq!(job.job_type, JobType::ProcessVideo);
        assert_eq!(job.state, JobState::Pending);
        assert!(job.idempotency_key.starts_with("user123:"));
    }

    #[test]
    fn test_job_state_transitions() {
        let job = Job::new_process_video("user123", "https://example.com", vec![Style::Original]);

        let started = job.start();
        assert_eq!(started.state, JobState::Processing);
        assert!(started.started_at.is_some());

        let completed = started.complete();
        assert_eq!(completed.state, JobState::Completed);
        assert_eq!(completed.progress, 100);
    }

    #[test]
    fn test_job_retry() {
        let job = Job::new_process_video("user123", "https://example.com", vec![Style::Original]);

        let failed = job.fail("Something went wrong");
        assert!(failed.can_retry());
        assert_eq!(failed.retry_count, 1);
    }
}
