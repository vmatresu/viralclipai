//! Clip metadata and task models.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AspectRatio, CropMode, Style, VideoId};

/// Status of a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipStatus {
    /// Clip is being processed
    #[default]
    Processing,
    /// Clip completed successfully
    Completed,
    /// Clip processing failed
    Failed,
}

impl ClipStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClipStatus::Processing => "processing",
            ClipStatus::Completed => "completed",
            ClipStatus::Failed => "failed",
        }
    }
}

/// Metadata for a processed clip stored in Firestore.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClipMetadata {
    /// Unique clip ID
    pub clip_id: String,

    /// Video ID this clip belongs to
    pub video_id: VideoId,

    /// User ID (owner)
    pub user_id: String,

    /// Scene ID (1-indexed)
    pub scene_id: u32,

    /// Scene title
    pub scene_title: String,

    /// Scene description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_description: Option<String>,

    /// Output filename
    pub filename: String,

    /// Style used
    pub style: String,

    /// Priority (lower = more important)
    #[serde(default = "default_priority")]
    pub priority: u32,

    /// Start timestamp
    pub start_time: String,

    /// End timestamp
    pub end_time: String,

    /// Duration in seconds
    pub duration_seconds: f64,

    /// File size in bytes
    #[serde(default)]
    pub file_size_bytes: u64,

    /// File size in MB
    #[serde(default)]
    pub file_size_mb: f64,

    /// Whether thumbnail exists
    #[serde(default)]
    pub has_thumbnail: bool,

    /// R2 key for the clip file
    pub r2_key: String,

    /// R2 key for the thumbnail
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_r2_key: Option<String>,

    /// R2 key for the raw (unstyled) segment before styling is applied.
    /// Multiple styled clips for the same scene can reference the same raw segment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_r2_key: Option<String>,

    /// Processing status
    #[serde(default)]
    pub status: ClipStatus,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Last update timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,

    /// Created by (user ID)
    pub created_by: String,
}

fn default_priority() -> u32 {
    99
}

impl ClipMetadata {
    /// Mark as completed with file info.
    pub fn complete(mut self, file_size_bytes: u64, has_thumbnail: bool) -> Self {
        self.status = ClipStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Some(Utc::now());
        self.file_size_bytes = file_size_bytes;
        self.file_size_mb = file_size_bytes as f64 / (1024.0 * 1024.0);
        self.has_thumbnail = has_thumbnail;
        self
    }

    /// Mark as failed.
    pub fn fail(mut self) -> Self {
        self.status = ClipStatus::Failed;
        self.updated_at = Some(Utc::now());
        self
    }
}

/// A task to create a single clip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClipTask {
    /// Scene ID (1-indexed)
    pub scene_id: u32,

    /// Scene title (sanitized for filename)
    pub scene_title: String,

    /// Scene description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_description: Option<String>,

    /// Start timestamp
    pub start: String,

    /// End timestamp
    pub end: String,

    /// Style to apply
    pub style: Style,

    /// Crop mode
    #[serde(default)]
    pub crop_mode: CropMode,

    /// Target aspect ratio
    #[serde(default)]
    pub target_aspect: AspectRatio,

    /// Priority (lower = more important, used in filename)
    #[serde(default = "default_priority")]
    pub priority: u32,

    /// Padding before start (seconds)
    #[serde(default)]
    pub pad_before: f64,

    /// Padding after end (seconds)
    #[serde(default)]
    pub pad_after: f64,
}

impl ClipTask {
    /// Create a new clip task.
    pub fn new(
        scene_id: u32,
        scene_title: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
        style: Style,
    ) -> Self {
        Self {
            scene_id,
            scene_title: scene_title.into(),
            scene_description: None,
            start: start.into(),
            end: end.into(),
            style,
            crop_mode: CropMode::default(),
            target_aspect: AspectRatio::default(),
            priority: 99,
            pad_before: 0.0,
            pad_after: 0.0,
        }
    }

    /// Generate the output filename.
    ///
    /// Format: `clip_{priority:02}_{safe_title}_{style}.mp4`
    pub fn output_filename(&self) -> String {
        let safe_title = sanitize_title(&self.scene_title);
        format!(
            "clip_{:02}_{}_{}_{}.mp4",
            self.priority,
            self.scene_id,
            safe_title,
            self.style.as_filename_part()
        )
    }
}

/// Sanitize a title for use in filenames.
fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .to_lowercase()
        .chars()
        .take(50) // Limit length
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_task_filename() {
        let task = ClipTask {
            scene_id: 1,
            scene_title: "My Amazing Scene!".to_string(),
            scene_description: None,
            start: "00:00:00".to_string(),
            end: "00:01:00".to_string(),
            style: Style::Split,
            crop_mode: CropMode::None,
            target_aspect: AspectRatio::PORTRAIT,
            priority: 1,
            pad_before: 0.0,
            pad_after: 0.0,
        };

        let filename = task.output_filename();
        assert!(filename.starts_with("clip_01_1_"));
        assert!(filename.ends_with("_split.mp4"));
    }

    #[test]
    fn test_sanitize_title() {
        assert_eq!(sanitize_title("Hello World!"), "hello_world");
        assert_eq!(sanitize_title("Test@#$%123"), "test123");
    }
}
