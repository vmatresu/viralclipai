//! WebSocket message types.
//!
//! These messages maintain compatibility with the existing Python API.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// WebSocket message types (matching Python implementation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WsMessageType {
    /// Log message
    Log,
    /// Progress update
    Progress,
    /// Error message
    Error,
    /// Processing complete
    Done,
    /// Clip uploaded notification
    ClipUploaded,
}

impl WsMessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WsMessageType::Log => "log",
            WsMessageType::Progress => "progress",
            WsMessageType::Error => "error",
            WsMessageType::Done => "done",
            WsMessageType::ClipUploaded => "clip_uploaded",
        }
    }
}

/// WebSocket message envelope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Log message with timestamp
    Log {
        message: String,
        timestamp: DateTime<Utc>,
    },

    /// Progress update (0-100)
    Progress { value: u8 },

    /// Error message
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// Processing complete
    Done {
        #[serde(rename = "videoId")]
        video_id: String,
    },

    /// Clip uploaded notification
    ClipUploaded {
        #[serde(rename = "videoId")]
        video_id: String,
        #[serde(rename = "clipCount")]
        clip_count: u32,
        #[serde(rename = "totalClips")]
        total_clips: u32,
        /// Credits consumed for this clip (defaults to 1 for backward compatibility).
        #[serde(default = "default_clip_credits", rename = "credits")]
        credits: u32,
    },

    /// Detailed clip processing progress
    ClipProgress {
        /// Scene ID being processed
        #[serde(rename = "sceneId")]
        scene_id: u32,
        /// Style being processed
        style: String,
        /// Current processing step
        step: ClipProcessingStep,
        /// Additional details about the step
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },

    /// Scene processing started
    SceneStarted {
        /// Scene ID
        #[serde(rename = "sceneId")]
        scene_id: u32,
        /// Scene title
        #[serde(rename = "sceneTitle")]
        scene_title: String,
        /// Number of styles being processed
        #[serde(rename = "styleCount")]
        style_count: u32,
        /// Start time in seconds
        #[serde(rename = "startSec")]
        start_sec: f64,
        /// Duration in seconds
        #[serde(rename = "durationSec")]
        duration_sec: f64,
    },

    /// Scene processing completed
    SceneCompleted {
        /// Scene ID
        #[serde(rename = "sceneId")]
        scene_id: u32,
        /// Number of clips completed
        #[serde(rename = "clipsCompleted")]
        clips_completed: u32,
        /// Number of clips failed
        #[serde(rename = "clipsFailed")]
        clips_failed: u32,
    },
}

/// Detailed clip processing step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipProcessingStep {
    /// Extracting video segment
    ExtractingSegment,
    /// Detecting faces in frames
    DetectingFaces,
    /// Face detection complete
    FaceDetectionComplete,
    /// Computing camera path
    ComputingCameraPath,
    /// Camera path complete
    CameraPathComplete,
    /// Computing crop windows
    ComputingCropWindows,
    /// Rendering output
    Rendering,
    /// Render complete
    RenderComplete,
    /// Uploading to storage
    Uploading,
    /// Upload complete
    UploadComplete,
    /// Clip processing complete
    Complete,
    /// Clip processing failed
    Failed,
}

impl WsMessage {
    /// Create a log message.
    pub fn log(message: impl Into<String>) -> Self {
        let now = Utc::now();
        WsMessage::Log {
            message: message.into(),
            timestamp: now,
        }
    }

    /// Create a progress message.
    pub fn progress(value: u8) -> Self {
        WsMessage::Progress {
            value: value.min(100),
        }
    }

    /// Create an error message.
    pub fn error(message: impl Into<String>) -> Self {
        let now = Utc::now();
        let ts = now.format("%H:%M:%S").to_string();
        let message = format!("[{}] {}", ts, message.into());
        WsMessage::Error {
            message,
            details: None,
            timestamp: now,
        }
    }

    /// Create an error message with details.
    pub fn error_with_details(message: impl Into<String>, details: impl Into<String>) -> Self {
        let now = Utc::now();
        let ts = now.format("%H:%M:%S").to_string();
        let message = format!("[{}] {}", ts, message.into());
        WsMessage::Error {
            message,
            details: Some(details.into()),
            timestamp: now,
        }
    }

    /// Create a done message.
    pub fn done(video_id: impl Into<String>) -> Self {
        WsMessage::Done {
            video_id: video_id.into(),
        }
    }

    /// Create a clip uploaded message.
    pub fn clip_uploaded(video_id: impl Into<String>, clip_count: u32, total_clips: u32) -> Self {
        WsMessage::ClipUploaded {
            video_id: video_id.into(),
            clip_count,
            total_clips,
            credits: default_clip_credits(),
        }
    }

    /// Create a clip uploaded message with explicit credit consumption.
    pub fn clip_uploaded_with_credits(
        video_id: impl Into<String>,
        clip_count: u32,
        total_clips: u32,
        credits: u32,
    ) -> Self {
        WsMessage::ClipUploaded {
            video_id: video_id.into(),
            clip_count,
            total_clips,
            credits,
        }
    }

    /// Create a clip progress message.
    pub fn clip_progress(
        scene_id: u32,
        style: impl Into<String>,
        step: ClipProcessingStep,
        details: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let ts = now.format("%H:%M:%S").to_string();
        let details = details.map(|d| format!("[{}] {}", ts, d));
        WsMessage::ClipProgress {
            scene_id,
            style: style.into(),
            step,
            details,
            timestamp: now,
        }
    }

    /// Create a scene started message.
    pub fn scene_started(
        scene_id: u32,
        scene_title: impl Into<String>,
        style_count: u32,
        start_sec: f64,
        duration_sec: f64,
    ) -> Self {
        WsMessage::SceneStarted {
            scene_id,
            scene_title: scene_title.into(),
            style_count,
            start_sec,
            duration_sec,
        }
    }

    /// Create a scene completed message.
    pub fn scene_completed(scene_id: u32, clips_completed: u32, clips_failed: u32) -> Self {
        WsMessage::SceneCompleted {
            scene_id,
            clips_completed,
            clips_failed,
        }
    }

    /// Get the message type.
    pub fn message_type(&self) -> WsMessageType {
        match self {
            WsMessage::Log { .. } => WsMessageType::Log,
            WsMessage::Progress { .. } => WsMessageType::Progress,
            WsMessage::Error { .. } => WsMessageType::Error,
            WsMessage::Done { .. } => WsMessageType::Done,
            WsMessage::ClipUploaded { .. } => WsMessageType::ClipUploaded,
            WsMessage::ClipProgress { .. } => WsMessageType::Progress,
            WsMessage::SceneStarted { .. } => WsMessageType::Progress,
            WsMessage::SceneCompleted { .. } => WsMessageType::Progress,
        }
    }
}

/// Request to process a video via WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WsProcessRequest {
    /// Firebase auth token
    pub token: String,

    /// Video URL
    pub url: String,

    /// Styles to apply (optional)
    #[serde(default)]
    pub styles: Option<Vec<String>>,

    /// Custom prompt (optional)
    #[serde(default)]
    pub prompt: Option<String>,

    /// Crop mode
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,

    /// Target aspect ratio
    #[serde(default = "default_aspect")]
    pub target_aspect: String,
}

fn default_crop_mode() -> String {
    "none".to_string()
}

fn default_aspect() -> String {
    "9:16".to_string()
}

fn default_clip_credits() -> u32 {
    1
}

/// Request to reprocess scenes via WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WsReprocessRequest {
    /// Firebase auth token
    pub token: String,

    /// Video ID to reprocess
    pub video_id: String,

    /// Scene IDs to reprocess
    pub scene_ids: Vec<u32>,

    /// Styles to apply
    pub styles: Vec<String>,

    /// Crop mode
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,

    /// Target aspect ratio
    #[serde(default = "default_aspect")]
    pub target_aspect: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::log("Hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"log\""));
        assert!(json.contains("\"message\":\"Hello\""));
    }

    #[test]
    fn test_ws_message_progress() {
        let msg = WsMessage::progress(150); // Should clamp to 100
        if let WsMessage::Progress { value } = msg {
            assert_eq!(value, 100);
        } else {
            panic!("Expected Progress message");
        }
    }

    #[test]
    fn test_ws_message_clip_uploaded() {
        let msg = WsMessage::clip_uploaded("video123", 5, 10);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"clipCount\":5"));
        assert!(json.contains("\"totalClips\":10"));
        assert!(json.contains("\"credits\":1"));
    }

    #[test]
    fn test_ws_message_clip_uploaded_with_credits() {
        let msg = WsMessage::clip_uploaded_with_credits("video123", 1, 4, 3);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"credits\":3"));
    }
}
