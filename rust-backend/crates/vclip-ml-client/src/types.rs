//! ML service request/response types.

use serde::{Deserialize, Serialize};
use vclip_models::AspectRatio;

/// Request for intelligent crop analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropRequest {
    /// Path to input video
    pub input_path: String,
    /// Target aspect ratio
    pub target_aspect: AspectRatio,
    /// Time range (start, end) in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<(f64, f64)>,
    /// Output prefix for rendered files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_prefix: Option<String>,
    /// Whether to save crop plan JSON
    #[serde(default)]
    pub save_crop_plan: bool,
}

/// Crop window for a frame range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropWindow {
    /// Frame number (or keyframe index)
    pub frame: u64,
    /// X position of crop window
    pub x: f64,
    /// Y position of crop window
    pub y: f64,
    /// Width of crop window
    pub width: f64,
    /// Height of crop window
    pub height: f64,
}

/// Crop plan for a video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropPlan {
    /// Video width
    pub video_width: u32,
    /// Video height
    pub video_height: u32,
    /// Video FPS
    pub fps: f64,
    /// Target aspect ratio
    pub target_aspect: AspectRatio,
    /// Crop windows (keyframes)
    pub windows: Vec<CropWindow>,
    /// Whether the crop is static (single window) or dynamic (multiple keyframes)
    pub is_static: bool,
}

/// Response from crop analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropResponse {
    /// Crop plan
    pub crop_plan: CropPlan,
    /// Output path (if rendered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: Option<String>,
}
