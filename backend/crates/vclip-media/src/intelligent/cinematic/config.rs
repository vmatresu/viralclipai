//! Configuration for the Cinematic pipeline.

use serde::{Deserialize, Serialize};

/// Trajectory optimization method for camera path smoothing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrajectoryMethod {
    /// L2 polynomial fitting - fast, good baseline.
    /// Produces paths with small, non-zero motion everywhere.
    L2Polynomial,

    /// L1 optimal paths - better quality, promotes sparsity in derivatives.
    /// Produces cinematographic segments: static → pan → static.
    /// This is the default as it produces more professional results.
    #[default]
    L1Optimal,
}

/// Configuration for the Cinematic pipeline.
///
/// This configuration controls all aspects of the AutoAI-inspired
/// cinematic camera motion system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CinematicConfig {
    // ============================================
    // Camera Mode Thresholds
    // ============================================
    /// Threshold for classifying as stationary mode (normalized, 0-1).
    /// If position std dev < this, camera locks to median position.
    pub stationary_threshold: f64,

    /// Threshold for classifying as panning vs tracking mode (normalized, 0-1).
    /// Position std dev > this triggers full tracking mode.
    pub panning_threshold: f64,

    // ============================================
    // Trajectory Optimization
    // ============================================
    /// Trajectory optimization method.
    /// L1Optimal (default) produces cinematographic segments with distinct static/pan periods.
    /// L2Polynomial is faster but produces constant micro-motion.
    pub trajectory_method: TrajectoryMethod,

    /// Degree of polynomial for trajectory fitting (used by L2Polynomial method).
    /// 3 = cubic (good balance of smooth vs responsive)
    pub polynomial_degree: usize,

    /// Regularization weight for smoothness.
    /// Higher values = smoother but less accurate tracking.
    /// Range: 0.0 (no smoothing) to 1.0 (very smooth)
    pub smoothness_weight: f64,

    /// Output sample rate in frames per second.
    pub output_sample_rate: f64,

    // ============================================
    // Adaptive Zoom
    // ============================================
    /// Minimum zoom level (1.0 = no zoom, frame as-is).
    pub min_zoom: f64,

    /// Maximum zoom level.
    pub max_zoom: f64,

    /// Ideal face ratio relative to frame height.
    /// 0.25 means face should be ~25% of frame height for tight framing.
    pub ideal_face_ratio: f64,

    /// Activity threshold to consider a face "active" (0-1).
    /// Faces with activity below this are considered background.
    pub multi_face_threshold: f64,

    /// Smoothing factor for zoom transitions (0-1).
    /// Lower values = faster zoom changes, higher = smoother.
    pub zoom_smoothing: f64,

    // ============================================
    // Face Margins (inherited from base config)
    // ============================================
    /// Vertical margin around face (as fraction of crop height).
    pub vertical_margin: f64,

    /// Horizontal margin around face (as fraction of crop width).
    pub horizontal_margin: f64,

    // ============================================
    // Detection Settings
    // ============================================
    /// Sample rate for face detection (fps).
    pub detection_fps: f64,

    /// Minimum face size as fraction of frame area.
    pub min_face_size: f64,

    // ============================================
    // Shot Detection (AutoAI-style)
    // ============================================
    /// Enable shot boundary detection.
    /// When true, each shot is processed independently for optimal camera motion.
    pub enable_shot_detection: bool,

    /// Chi-squared distance threshold for shot boundary detection.
    /// Higher = fewer cuts detected, lower = more sensitive.
    /// Range: 0.3 (sensitive) to 1.0 (conservative)
    pub shot_threshold: f64,

    /// Minimum shot duration in seconds.
    /// Shots shorter than this are merged with adjacent shots.
    pub min_shot_duration: f64,

    /// Sample FPS for shot detection (separate from face detection fps).
    pub shot_detection_fps: f64,

    // ============================================
    // Object Detection
    // ============================================
    /// Enable object detection for improved camera motion in scenes without faces.
    /// When enabled, uses YOLOv8 to detect objects and fuse them with face signals.
    pub enable_object_detection: bool,

    // ============================================
    // Multi-Frame Scene Analysis
    // ============================================
    /// Number of frames to analyze ahead before making camera decisions.
    /// Higher = smoother but less responsive. Default: 15 (0.5s at 30fps).
    pub lookahead_frames: usize,

    // ============================================
    // Signal Fusion Weights
    // ============================================
    /// Weight for face signals in saliency fusion (default: 1.0).
    pub face_weight: f64,

    /// Activity boost factor - faces with high mouth activity get boosted weight.
    pub activity_boost: f64,

    /// Weight for object (non-face) signals. Person objects get 1.5x this weight.
    pub object_weight: f64,

    /// Padding around required signals (fraction, e.g., 0.2 = 20%).
    pub signal_padding: f64,
}

impl Default for CinematicConfig {
    fn default() -> Self {
        Self {
            // Camera mode thresholds
            stationary_threshold: 0.05,
            panning_threshold: 0.15,

            // Trajectory optimization
            trajectory_method: TrajectoryMethod::default(), // L1Optimal
            polynomial_degree: 3,
            smoothness_weight: 0.3,
            output_sample_rate: 30.0,

            // Adaptive zoom - conservative values to avoid cutting faces
            // Reduced further from 1.8 max_zoom to prevent over-zoom
            min_zoom: 1.0,
            max_zoom: 1.4,          // Reduced from 1.8 to prevent over-zoom
            ideal_face_ratio: 0.12, // Reduced from 0.18 - face ~12% of frame (wider framing)
            multi_face_threshold: 0.3,
            zoom_smoothing: 0.15, // Smooth zoom transitions

            // Face margins - generous for better face containment
            vertical_margin: 0.20,   // Give more headroom
            horizontal_margin: 0.15, // Side margins

            // Detection settings
            detection_fps: 8.0,
            min_face_size: 0.02,

            // Shot detection (enabled by default for cinematic)
            enable_shot_detection: true,
            shot_threshold: 0.5,
            min_shot_duration: 0.5,
            shot_detection_fps: 5.0,

            // Object detection (disabled by default for better camera motion)
            enable_object_detection: false,

            // Multi-frame scene analysis
            lookahead_frames: 15, // 0.5s at 30fps

            // Signal fusion
            face_weight: 1.0,
            activity_boost: 0.5,
            object_weight: 0.3,   // Weight for object detections
            signal_padding: 0.45, // More generous padding around subjects
        }
    }
}

impl CinematicConfig {
    /// Create a configuration optimized for fast processing.
    pub fn fast() -> Self {
        Self {
            detection_fps: 4.0,
            output_sample_rate: 24.0,
            polynomial_degree: 2, // Lower degree = faster
            ..Default::default()
        }
    }

    /// Create a configuration optimized for quality.
    pub fn quality() -> Self {
        Self {
            detection_fps: 10.0,
            output_sample_rate: 30.0,
            polynomial_degree: 4, // Higher degree = smoother
            smoothness_weight: 0.4,
            ..Default::default()
        }
    }

    /// Create a configuration optimized for interviews/podcasts.
    pub fn podcast() -> Self {
        Self {
            // More conservative thresholds for stable framing
            stationary_threshold: 0.08,
            panning_threshold: 0.20,
            // Tighter framing for talking heads
            ideal_face_ratio: 0.30,
            // Slower zoom transitions
            zoom_smoothing: 0.15,
            ..Default::default()
        }
    }

    /// Builder: Set camera mode thresholds.
    pub fn with_camera_thresholds(mut self, stationary: f64, panning: f64) -> Self {
        self.stationary_threshold = stationary;
        self.panning_threshold = panning;
        self
    }

    /// Builder: Set polynomial degree.
    pub fn with_polynomial_degree(mut self, degree: usize) -> Self {
        self.polynomial_degree = degree;
        self
    }

    /// Builder: Set smoothness weight.
    pub fn with_smoothness(mut self, weight: f64) -> Self {
        self.smoothness_weight = weight;
        self
    }

    /// Builder: Set zoom limits.
    pub fn with_zoom_limits(mut self, min: f64, max: f64) -> Self {
        self.min_zoom = min;
        self.max_zoom = max;
        self
    }

    /// Builder: Set ideal face ratio.
    pub fn with_ideal_face_ratio(mut self, ratio: f64) -> Self {
        self.ideal_face_ratio = ratio;
        self
    }

    /// Builder: Set detection FPS.
    pub fn with_detection_fps(mut self, fps: f64) -> Self {
        self.detection_fps = fps;
        self
    }

    /// Builder: Set trajectory optimization method.
    pub fn with_trajectory_method(mut self, method: TrajectoryMethod) -> Self {
        self.trajectory_method = method;
        self
    }

    /// Builder: Enable/disable object detection.
    pub fn with_object_detection(mut self, enabled: bool) -> Self {
        self.enable_object_detection = enabled;
        self
    }

    /// Builder: Use L2 polynomial trajectory (faster, less cinematic).
    pub fn with_l2_trajectory(self) -> Self {
        self.with_trajectory_method(TrajectoryMethod::L2Polynomial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CinematicConfig::default();
        assert_eq!(config.polynomial_degree, 3);
        assert!((config.smoothness_weight - 0.3).abs() < 0.001);
        assert!((config.stationary_threshold - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_fast_config() {
        let config = CinematicConfig::fast();
        assert_eq!(config.polynomial_degree, 2);
        assert!((config.detection_fps - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_quality_config() {
        let config = CinematicConfig::quality();
        assert_eq!(config.polynomial_degree, 4);
        assert!((config.detection_fps - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_builder_pattern() {
        let config = CinematicConfig::default()
            .with_polynomial_degree(5)
            .with_smoothness(0.5)
            .with_zoom_limits(1.5, 2.5);

        assert_eq!(config.polynomial_degree, 5);
        assert!((config.smoothness_weight - 0.5).abs() < 0.001);
        assert!((config.min_zoom - 1.5).abs() < 0.001);
        assert!((config.max_zoom - 2.5).abs() < 0.001);
    }
}
