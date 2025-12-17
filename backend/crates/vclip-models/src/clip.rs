//! Clip metadata and task models.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AspectRatio, CropMode, Style, VideoId};

/// Horizontal position for StreamerSplit bottom panel crop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalPosition {
    Left,
    #[default]
    Center,
    Right,
}

impl HorizontalPosition {
    /// Returns the normalized X position (0.0 = left, 0.5 = center, 1.0 = right).
    pub fn to_normalized(&self) -> f64 {
        match self {
            HorizontalPosition::Left => 0.15,    // 15% from left
            HorizontalPosition::Center => 0.50,  // Center
            HorizontalPosition::Right => 0.85,   // 15% from right
        }
    }
}

/// Vertical position for StreamerSplit bottom panel crop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerticalPosition {
    #[default]
    Top,
    Middle,
    Bottom,
}

impl VerticalPosition {
    /// Returns the normalized Y position (0.0 = top, 0.5 = middle, 1.0 = bottom).
    pub fn to_normalized(&self) -> f64 {
        match self {
            VerticalPosition::Top => 0.20,      // 20% from top (typical webcam)
            VerticalPosition::Middle => 0.50,   // Center
            VerticalPosition::Bottom => 0.80,   // 20% from bottom
        }
    }
}

/// Parameters for StreamerSplit style - user-specified crop for bottom panel.
///
/// This allows users to manually select where to crop the bottom panel
/// instead of relying on face detection (which can be unreliable for
/// gaming content with multiple faces/avatars).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StreamerSplitParams {
    /// Horizontal position of the crop area.
    #[serde(default)]
    pub position_x: HorizontalPosition,

    /// Vertical position of the crop area.
    #[serde(default)]
    pub position_y: VerticalPosition,

    /// Zoom level for the bottom panel (1.0 = no zoom, 2.0 = 2x zoom, max 4.0).
    /// Default is 1.5 for a slight zoom on webcam overlays.
    #[serde(default = "default_zoom")]
    pub zoom: f32,

    /// Optional static image URL to display in the bottom panel instead of video crop.
    /// If provided, the video crop is ignored and this image is shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_image_url: Option<String>,
}

fn default_zoom() -> f32 {
    1.5
}

impl Default for StreamerSplitParams {
    fn default() -> Self {
        Self {
            position_x: HorizontalPosition::Left,  // Top-left is most common webcam position
            position_y: VerticalPosition::Top,
            zoom: 1.5,
            static_image_url: None,
        }
    }
}

/// Parameters for Streamer full-view style - landscape video centered with blurred background.
///
/// This creates a 9:16 portrait output with the original landscape video centered
/// and a blurred/zoomed version of the same video as the background.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct StreamerParams {
    /// Enable Top Scenes compilation mode.
    /// When enabled, creates a compilation of up to 5 scenes with countdown overlay.
    #[serde(default)]
    pub top_scenes_enabled: bool,

    /// Scene timestamps for Top Scenes compilation (max 5).
    /// Each entry is a tuple of (start_time, end_time) in seconds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_scenes: Vec<TopSceneEntry>,
}

/// A single scene entry for Top Scenes compilation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TopSceneEntry {
    /// Scene number (for countdown display, e.g., 5, 4, 3, 2, 1)
    pub scene_number: u8,
    /// Start timestamp in "HH:MM:SS" or seconds format
    pub start: String,
    /// End timestamp in "HH:MM:SS" or seconds format
    pub end: String,
    /// Scene title (optional, for overlay text)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl StreamerParams {
    /// Create default Streamer params (single scene, no Top Scenes).
    pub fn single() -> Self {
        Self {
            top_scenes_enabled: false,
            top_scenes: Vec::new(),
        }
    }

    /// Create Top Scenes params with the given scenes.
    pub fn top_scenes(scenes: Vec<TopSceneEntry>) -> Self {
        Self {
            top_scenes_enabled: true,
            top_scenes: scenes.into_iter().take(5).collect(), // Max 5 scenes
        }
    }
}

impl StreamerSplitParams {
    /// Create params for top-left corner (most common webcam position for gaming).
    pub fn top_left() -> Self {
        Self {
            position_x: HorizontalPosition::Left,
            position_y: VerticalPosition::Top,
            zoom: 2.0,
            static_image_url: None,
        }
    }

    /// Create params for top-right corner.
    pub fn top_right() -> Self {
        Self {
            position_x: HorizontalPosition::Right,
            position_y: VerticalPosition::Top,
            zoom: 2.0,
            static_image_url: None,
        }
    }

    /// Create params for center (full frame, no zoom).
    pub fn full_frame() -> Self {
        Self {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 1.0,
            static_image_url: None,
        }
    }

    /// Create params with a static image.
    pub fn with_static_image(url: impl Into<String>) -> Self {
        Self {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 1.0,
            static_image_url: Some(url.into()),
        }
    }
}

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

    /// Optional parameters for StreamerSplit style.
    /// If not provided, defaults to top-left position with 1.5x zoom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamer_split_params: Option<StreamerSplitParams>,

    /// Optional parameters for Streamer (full-view) style.
    /// Used for Top Scenes compilation and other Streamer-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamer_params: Option<StreamerParams>,

    /// Whether to cut silent parts using VAD (default: true)
    #[serde(default = "default_cut_silent_parts")]
    pub cut_silent_parts: bool,
}

fn default_cut_silent_parts() -> bool {
    true
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
            streamer_split_params: None,
            streamer_params: None,
            cut_silent_parts: true,
        }
    }

    /// Set Streamer parameters.
    pub fn with_streamer_params(mut self, params: StreamerParams) -> Self {
        self.streamer_params = Some(params);
        self
    }

    /// Set StreamerSplit parameters.
    pub fn with_streamer_split_params(mut self, params: StreamerSplitParams) -> Self {
        self.streamer_split_params = Some(params);
        self
    }

    /// Generate the output filename.
    ///
    /// Format: `clip_{priority:02}_{safe_title}_{style}.mp4`
    pub fn output_filename(&self) -> String {
        let safe_title = sanitize_filename_title(&self.scene_title);
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
///
/// Only allows ASCII alphanumeric, hyphen, underscore, and space.
/// Non-ASCII characters (including Unicode letters like 'î', 'ă', 'ș') are
/// stripped to prevent URL encoding mismatches between R2 and signed URLs.
pub fn sanitize_filename_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
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
        assert_eq!(sanitize_filename_title("Hello World!"), "hello_world");
        assert_eq!(sanitize_filename_title("Test@#$%123"), "test123");
    }

    #[test]
    fn test_sanitize_title_unicode() {
        // Romanian characters should be stripped (prevents 404 from URL encoding)
        assert_eq!(sanitize_filename_title("Soluția românească"), "soluia_romneasc");
        assert_eq!(sanitize_filename_title("RAM-ii și cartofii"), "ram-ii_i_cartofii");
        // Only ASCII alphanumeric + space/hyphen/underscore allowed
        assert_eq!(sanitize_filename_title("Café résumé"), "caf_rsum");
    }
}
