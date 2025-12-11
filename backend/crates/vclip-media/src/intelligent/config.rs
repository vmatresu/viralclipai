//! Configuration for the intelligent cropping pipeline.
//!
//! Mirrors the Python IntelligentCropConfig.

use serde::{Deserialize, Serialize};

/// Configuration for the intelligent cropping pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligentCropConfig {
    // === Analysis Settings ===
    /// Frames per second to sample for analysis (default: 8.0 for responsive tracking)
    pub fps_sample: f64,

    /// Resolution (height) to use for analysis (default: 480)
    pub analysis_resolution: u32,

    /// Resolution for face detection neural network input (default: 320 for fast detection)
    pub detection_resolution: u32,

    // === Face Detection ===
    /// Minimum confidence for face detection (default: 0.5)
    pub min_detection_confidence: f64,

    /// Minimum face size as fraction of frame area (default: 0.02)
    pub min_face_size: f64,

    /// Expand detected face box by this ratio (default: 0.3)
    pub face_expand_ratio: f64,

    // === Tracking ===
    /// IoU threshold for track matching (default: 0.3)
    pub iou_threshold: f64,

    /// Maximum frames to maintain a track without detection (default: 10)
    pub max_track_gap: u32,

    // === Composition ===
    /// Target headroom as fraction of crop height (default: 0.15)
    pub headroom_ratio: f64,

    /// Padding around subject as fraction of subject size (default: 0.2)
    pub subject_padding: f64,

    /// Minimum margin from crop edge as fraction of crop size (default: 0.05)
    pub safe_margin: f64,

    // === Camera Smoothing ===
    /// Maximum virtual camera pan speed in pixels per second (default: 200.0)
    pub max_pan_speed: f64,

    /// Smoothing window duration in seconds (default: 0.5)
    pub smoothing_window: f64,

    // === Zoom Limits ===
    /// Maximum zoom factor relative to source (default: 3.0)
    pub max_zoom_factor: f64,

    /// Minimum zoom factor (default: 1.0)
    pub min_zoom_factor: f64,

    // === Multi-Subject Handling ===
    /// Prefer following primary subject over group framing (default: true)
    pub prefer_primary_subject: bool,

    /// Distance threshold for faces to be considered "far apart" (default: 0.4)
    pub multi_face_separation_threshold: f64,

    // === Fallback ===
    /// Fallback policy when no faces detected
    pub fallback_policy: FallbackPolicy,

    // === Rendering ===
    /// FFmpeg x264 preset for rendering (default: "veryfast")
    pub render_preset: String,

    /// FFmpeg CRF quality (default: 20)
    pub render_crf: u32,

    // === Face Activity Detection ===
    /// Enable mouth movement detection (requires LBF landmark model)
    pub enable_mouth_detection: bool,

    /// Time window for aggregating activity scores (seconds, default: 0.5)
    pub face_activity_window: f64,

    /// Minimum duration before switching active face (seconds, default: 1.0)
    pub min_switch_duration: f64,

    /// Activity score margin required to switch faces (default: 0.2 = 20%)
    pub switch_margin: f64,

    /// Weight for mouth activity in combined score (default: 1.0, visual-only)
    pub activity_weight_mouth: f64,

    /// Weight for motion activity in combined score (default: 0.0, disabled)
    pub activity_weight_motion: f64,

    /// Weight for size changes in combined score (default: 0.0, disabled)
    pub activity_weight_size_change: f64,

    /// EMA smoothing parameter for activity scores (default: 0.3)
    pub activity_smoothing_window: f64,
}

/// Policy when no faces are detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackPolicy {
    /// Center crop
    Center,
    /// Upper-center (TikTok style)
    UpperCenter,
    /// Rule of thirds composition
    RuleOfThirds,
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self::UpperCenter
    }
}

impl Default for IntelligentCropConfig {
    fn default() -> Self {
        Self {
            // Analysis - 8 fps for responsive speaker tracking
            fps_sample: 8.0,
            analysis_resolution: 480,
            detection_resolution: 320,

            // Face Detection
            min_detection_confidence: 0.5,
            min_face_size: 0.02,
            face_expand_ratio: 0.3,

            // Tracking
            iou_threshold: 0.3,
            max_track_gap: 10,

            // Composition
            headroom_ratio: 0.15,
            subject_padding: 0.2,
            safe_margin: 0.05,

            // Camera Smoothing - increased pan speed for responsive speaker switching
            max_pan_speed: 600.0,   // Faster transitions between speakers
            smoothing_window: 0.3,  // Shorter smoothing for snappier movement

            // Zoom Limits
            max_zoom_factor: 3.0,
            min_zoom_factor: 1.0,

            // Multi-Subject
            prefer_primary_subject: true,
            multi_face_separation_threshold: 0.4,

            // Fallback
            fallback_policy: FallbackPolicy::UpperCenter,

            // Rendering - use CRF 24 to match EncodingConfig::for_split_view()
            // and keep file sizes reasonable (~4-6MB for 30s clip)
            render_preset: "fast".to_string(),
            render_crf: 24,

            // Face Activity Detection
            enable_mouth_detection: true,
            face_activity_window: 0.5,
            min_switch_duration: 1.0,
            switch_margin: 0.2,
            activity_weight_mouth: 1.0,
            activity_weight_motion: 0.0,
            activity_weight_size_change: 0.0,
            activity_smoothing_window: 0.3,
        }
    }
}

impl IntelligentCropConfig {
    /// Fast configuration for quick previews.
    pub fn fast() -> Self {
        Self {
            fps_sample: 2.0,
            analysis_resolution: 360,
            render_preset: "ultrafast".to_string(),
            render_crf: 23,
            ..Default::default()
        }
    }

    /// Quality configuration for final output.
    pub fn quality() -> Self {
        Self {
            fps_sample: 5.0,
            analysis_resolution: 720,
            render_preset: "slow".to_string(),
            render_crf: 18,
            smoothing_window: 0.8,
            ..Default::default()
        }
    }

    /// TikTok-optimized configuration.
    pub fn tiktok() -> Self {
        Self {
            fallback_policy: FallbackPolicy::UpperCenter,
            headroom_ratio: 0.12,
            subject_padding: 0.25,
            ..Default::default()
        }
    }

    /// Responsive configuration optimized for podcast speaker tracking.
    /// Prioritizes fast detection and speaker switching.
    pub fn responsive() -> Self {
        Self {
            fps_sample: 10.0,              // High sample rate for responsive tracking
            detection_resolution: 240,     // Small resolution for fast detection
            analysis_resolution: 360,      // Lower analysis resolution
            smoothing_window: 0.25,        // Fast smoothing for quick transitions
            max_pan_speed: 400.0,          // Higher pan speed for snappier movement
            render_preset: "veryfast".to_string(),
            ..Default::default()
        }
    }

    /// Premium configuration for intelligent_speaker style.
    /// Uses the enhanced premium camera planner with:
    /// - Dead-zone hysteresis for stability
    /// - Vertical bias for eye placement
    /// - Multi-speaker dwell time to prevent ping-ponging
    pub fn premium_speaker() -> Self {
        Self {
            fps_sample: 8.0,               // Good balance of responsiveness and stability
            analysis_resolution: 480,
            detection_resolution: 320,
            smoothing_window: 0.4,         // Moderate smoothing
            max_pan_speed: 400.0,          // Balanced pan speed
            min_switch_duration: 1.2,      // Longer dwell time for stability
            switch_margin: 0.25,           // Higher margin to prevent rapid switching
            headroom_ratio: 0.12,          // Good headroom for vertical framing
            subject_padding: 0.15,         // Moderate padding around subject
            render_preset: "fast".to_string(),
            render_crf: 22,
            ..Default::default()
        }
    }
}
