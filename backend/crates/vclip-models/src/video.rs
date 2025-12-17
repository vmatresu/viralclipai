//! Video metadata models.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

use crate::utils::extract_youtube_id_legacy;

/// Unique identifier for a video processing run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct VideoId(pub String);

impl VideoId {
    /// Generate a new random video ID.
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

impl Default for VideoId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VideoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for VideoId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for VideoId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Video processing status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VideoStatus {
    /// Video is being processed
    #[default]
    Processing,
    /// Analysis completed, scenes ready for selection (no clips rendered yet)
    Analyzed,
    /// Processing completed successfully (clips rendered)
    Completed,
    /// Processing failed
    Failed,
}

impl VideoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VideoStatus::Processing => "processing",
            VideoStatus::Analyzed => "analyzed",
            VideoStatus::Completed => "completed",
            VideoStatus::Failed => "failed",
        }
    }
}

impl fmt::Display for VideoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Source video background download status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceVideoStatus {
    /// Download not yet started
    #[default]
    Pending,
    /// Download in progress
    Downloading,
    /// Downloaded and stored in R2
    Ready,
    /// Source video expired (past TTL)
    Expired,
    /// Download failed
    Failed,
}

impl SourceVideoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceVideoStatus::Pending => "pending",
            SourceVideoStatus::Downloading => "downloading",
            SourceVideoStatus::Ready => "ready",
            SourceVideoStatus::Expired => "expired",
            SourceVideoStatus::Failed => "failed",
        }
    }
}

impl fmt::Display for SourceVideoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Processing progress for a video (stored in Firestore for frontend polling).
///
/// This replaces WebSocket-based real-time progress reporting.
/// The worker updates this at key intervals:
/// - Job start: Initialize with total scenes/clips
/// - After each scene: Update completed counts
/// - Job end: Clear progress (status set separately)
/// - On error: Set error message
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProcessingProgress {
    /// Total number of scenes to process
    pub total_scenes: u32,
    /// Number of scenes completed
    pub completed_scenes: u32,
    /// Total number of clips to generate
    pub total_clips: u32,
    /// Number of clips successfully completed
    pub completed_clips: u32,
    /// Number of clips that failed
    pub failed_clips: u32,
    /// Current scene being processed (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_scene_id: Option<u32>,
    /// Title of current scene being processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_scene_title: Option<String>,
    /// When processing started
    pub started_at: DateTime<Utc>,
    /// Last progress update
    pub updated_at: DateTime<Utc>,
    /// Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl ProcessingProgress {
    /// Create a new progress tracker for a processing job.
    pub fn new(total_scenes: u32, total_clips: u32) -> Self {
        let now = Utc::now();
        Self {
            total_scenes,
            completed_scenes: 0,
            total_clips,
            completed_clips: 0,
            failed_clips: 0,
            current_scene_id: None,
            current_scene_title: None,
            started_at: now,
            updated_at: now,
            error_message: None,
        }
    }

    /// Update progress when a scene starts.
    pub fn scene_started(&mut self, scene_id: u32, scene_title: Option<String>) {
        self.current_scene_id = Some(scene_id);
        self.current_scene_title = scene_title;
        self.updated_at = Utc::now();
    }

    /// Update progress when a scene completes.
    pub fn scene_completed(&mut self, clips_completed: u32, clips_failed: u32) {
        self.completed_scenes += 1;
        self.completed_clips += clips_completed;
        self.failed_clips += clips_failed;
        self.current_scene_id = None;
        self.current_scene_title = None;
        self.updated_at = Utc::now();
    }

    /// Set error message.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
        self.updated_at = Utc::now();
    }

    /// Calculate progress percentage (0-100).
    pub fn percentage(&self) -> u32 {
        if self.total_clips == 0 {
            return 0;
        }
        ((self.completed_clips as f64 / self.total_clips as f64) * 100.0) as u32
    }
}

/// Video metadata stored in Firestore.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VideoMetadata {
    /// Unique video ID
    pub video_id: VideoId,

    /// User ID (owner)
    pub user_id: String,

    /// Original video URL (YouTube, etc.)
    pub video_url: String,

    /// Video title
    pub video_title: String,

    /// YouTube video ID (extracted from URL)
    #[serde(default)]
    pub youtube_id: String,

    /// Processing status
    #[serde(default)]
    pub status: VideoStatus,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Failure timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<DateTime<Utc>>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Number of highlights detected
    #[serde(default)]
    pub highlights_count: u32,

    /// Custom prompt used for AI analysis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,

    /// Styles processed
    #[serde(default)]
    pub styles_processed: Vec<String>,

    /// Crop mode used
    #[serde(default)]
    pub crop_mode: String,

    /// Target aspect ratio
    #[serde(default = "default_aspect")]
    pub target_aspect: String,

    /// Number of clips generated
    #[serde(default)]
    pub clips_count: u32,

    /// Total size of all clips in bytes
    #[serde(default)]
    pub total_size_bytes: u64,

    /// Clips grouped by style
    #[serde(default)]
    pub clips_by_style: HashMap<String, u32>,

    /// R2 key for highlights.json
    pub highlights_json_key: String,

    /// Created by (user ID)
    pub created_by: String,

    // === Source Video Fields (Phase 2.2) ===

    /// R2 key for cached source video
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_video_r2_key: Option<String>,

    /// Source video background download status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_video_status: Option<SourceVideoStatus>,

    /// Expiration timestamp for cached source video
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_video_expires_at: Option<DateTime<Utc>>,

    /// Error message if source video download failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_video_error: Option<String>,

    // === Processing Progress Fields ===

    /// Current processing progress (for frontend polling).
    /// Set when processing starts, cleared when complete/failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_progress: Option<ProcessingProgress>,
}

fn default_aspect() -> String {
    "9:16".to_string()
}

impl VideoMetadata {
    /// Create a new video metadata record.
    pub fn new(
        video_id: VideoId,
        user_id: impl Into<String>,
        video_url: impl Into<String>,
        video_title: impl Into<String>,
    ) -> Self {
        let user_id = user_id.into();
        let video_url = video_url.into();
        let now = Utc::now();

        Self {
            video_id: video_id.clone(),
            user_id: user_id.clone(),
            video_url: video_url.clone(),
            video_title: video_title.into(),
            youtube_id: extract_youtube_id_legacy(&video_url).unwrap_or_default(),
            status: VideoStatus::Processing,
            created_at: now,
            updated_at: now,
            completed_at: None,
            failed_at: None,
            error_message: None,
            highlights_count: 0,
            custom_prompt: None,
            styles_processed: Vec::new(),
            crop_mode: "none".to_string(),
            target_aspect: "9:16".to_string(),
            clips_count: 0,
            total_size_bytes: 0,
            clips_by_style: HashMap::new(),
            highlights_json_key: format!("{}/{}/highlights.json", user_id, video_id),
            created_by: user_id,
            // Source video fields (Phase 2.2) - initialized as None
            source_video_r2_key: None,
            source_video_status: None,
            source_video_expires_at: None,
            source_video_error: None,
            // Processing progress - initialized as None
            processing_progress: None,
        }
    }

    /// Mark as completed.
    pub fn complete(mut self) -> Self {
        self.status = VideoStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self
    }

    /// Mark as failed.
    pub fn fail(mut self, error: impl Into<String>) -> Self {
        self.status = VideoStatus::Failed;
        self.failed_at = Some(Utc::now());
        self.error_message = Some(error.into());
        self.updated_at = Utc::now();
        self
    }
}

/// Summary of a video in user's library (for list view).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VideoSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,
    #[serde(default)]
    pub clips_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_id_generation() {
        let id1 = VideoId::new();
        let id2 = VideoId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_video_metadata_creation() {
        let id = VideoId::new();
        let meta = VideoMetadata::new(
            id.clone(),
            "user123",
            "https://youtube.com/watch?v=abc123def45",
            "Test Video",
        );

        assert_eq!(meta.video_id, id);
        assert_eq!(meta.status, VideoStatus::Processing);
        assert_eq!(meta.youtube_id, "abc123def45");
    }
}
