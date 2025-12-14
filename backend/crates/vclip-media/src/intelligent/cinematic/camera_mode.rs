//! Camera mode analysis for cinematic pipeline.
//!
//! This module implements AutoAI-inspired camera mode selection that analyzes
//! motion patterns to choose between stationary, panning, and tracking modes.

use super::config::CinematicConfig;
use crate::intelligent::CameraKeyframe;

/// Camera behavior mode for a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    /// Lock camera to median position (minimal motion in segment).
    /// Best for: static interviews, presentations, single speaker
    Stationary,

    /// Smooth linear pan from start to end position (moderate motion).
    /// Best for: slow camera movements, transitional shots
    Panning,

    /// Active tracking with full polynomial smoothing (high motion).
    /// Best for: action, multiple speakers, dynamic content
    Tracking,
}

impl CameraMode {
    /// Human-readable description of the mode.
    pub fn description(&self) -> &'static str {
        match self {
            CameraMode::Stationary => "Stationary (locked position)",
            CameraMode::Panning => "Panning (linear interpolation)",
            CameraMode::Tracking => "Tracking (polynomial smoothing)",
        }
    }
}

/// Analyzer for determining camera mode from motion statistics.
pub struct CameraModeAnalyzer {
    /// Threshold below which motion is considered "stationary".
    pub stationary_threshold: f64,

    /// Threshold above which motion triggers full tracking.
    pub panning_threshold: f64,
}

impl CameraModeAnalyzer {
    /// Create analyzer with thresholds from config.
    pub fn new(config: &CinematicConfig) -> Self {
        Self {
            stationary_threshold: config.stationary_threshold,
            panning_threshold: config.panning_threshold,
        }
    }

    /// Create analyzer with explicit thresholds.
    pub fn with_thresholds(stationary: f64, panning: f64) -> Self {
        Self {
            stationary_threshold: stationary,
            panning_threshold: panning,
        }
    }

    /// Analyze keyframes and determine the appropriate camera mode.
    ///
    /// # Arguments
    /// * `keyframes` - Camera keyframes from face detection/tracking
    /// * `frame_width` - Source video width for normalization
    /// * `frame_height` - Source video height for normalization
    ///
    /// # Returns
    /// The recommended `CameraMode` based on motion analysis.
    pub fn analyze(
        &self,
        keyframes: &[CameraKeyframe],
        frame_width: u32,
        frame_height: u32,
    ) -> CameraMode {
        if keyframes.is_empty() {
            return CameraMode::Stationary;
        }

        if keyframes.len() == 1 {
            return CameraMode::Stationary;
        }

        let w = frame_width as f64;
        let h = frame_height as f64;

        // Normalize positions to [0, 1] range
        let cx_values: Vec<f64> = keyframes.iter().map(|k| k.cx / w).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|k| k.cy / h).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|k| k.width / w).collect();

        // Compute standard deviations
        let cx_std = std_dev(&cx_values);
        let cy_std = std_dev(&cy_values);
        let width_std = std_dev(&width_values);

        // Combine position std devs
        let position_std = (cx_std + cy_std) / 2.0;
        let size_std = width_std;

        tracing::debug!(
            "Camera mode analysis: position_std={:.4}, size_std={:.4}",
            position_std,
            size_std
        );

        // Classification logic (from AutoAI)
        if position_std < self.stationary_threshold && size_std < self.stationary_threshold {
            CameraMode::Stationary
        } else if position_std > self.panning_threshold || size_std > self.panning_threshold * 0.67
        {
            CameraMode::Tracking
        } else {
            CameraMode::Panning
        }
    }

    /// Compute motion statistics for debugging/logging.
    pub fn compute_motion_stats(
        &self,
        keyframes: &[CameraKeyframe],
        frame_width: u32,
        frame_height: u32,
    ) -> MotionStats {
        if keyframes.is_empty() {
            return MotionStats::default();
        }

        let w = frame_width as f64;
        let h = frame_height as f64;

        let cx_values: Vec<f64> = keyframes.iter().map(|k| k.cx / w).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|k| k.cy / h).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|k| k.width / w).collect();

        MotionStats {
            cx_std: std_dev(&cx_values),
            cy_std: std_dev(&cy_values),
            width_std: std_dev(&width_values),
            cx_range: range(&cx_values),
            cy_range: range(&cy_values),
            width_range: range(&width_values),
            keyframe_count: keyframes.len(),
        }
    }
}

/// Motion statistics for a segment.
#[derive(Debug, Clone, Default)]
pub struct MotionStats {
    /// Standard deviation of normalized cx positions.
    pub cx_std: f64,
    /// Standard deviation of normalized cy positions.
    pub cy_std: f64,
    /// Standard deviation of normalized widths.
    pub width_std: f64,
    /// Range (max - min) of normalized cx positions.
    pub cx_range: f64,
    /// Range (max - min) of normalized cy positions.
    pub cy_range: f64,
    /// Range (max - min) of normalized widths.
    pub width_range: f64,
    /// Number of keyframes analyzed.
    pub keyframe_count: usize,
}

/// Compute standard deviation of a slice.
fn std_dev(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt()
}

/// Compute range (max - min) of a slice.
fn range(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    max - min
}

/// Compute median of a slice.
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keyframes(positions: &[(f64, f64, f64)]) -> Vec<CameraKeyframe> {
        positions
            .iter()
            .enumerate()
            .map(|(i, (cx, cy, w))| CameraKeyframe {
                time: i as f64 * 0.1,
                cx: *cx,
                cy: *cy,
                width: *w,
                height: *w * 0.5625, // 16:9 aspect
            })
            .collect()
    }

    #[test]
    fn test_stationary_mode() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);

        // Very stable positions (all nearly identical)
        let keyframes = make_keyframes(&[
            (500.0, 300.0, 200.0),
            (502.0, 301.0, 201.0),
            (498.0, 299.0, 199.0),
            (501.0, 300.0, 200.0),
        ]);

        let mode = analyzer.analyze(&keyframes, 1920, 1080);
        assert_eq!(mode, CameraMode::Stationary);
    }

    #[test]
    fn test_tracking_mode() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);

        // Large movement across frame
        let keyframes = make_keyframes(&[
            (200.0, 300.0, 200.0),
            (600.0, 350.0, 250.0),
            (1000.0, 400.0, 300.0),
            (1400.0, 500.0, 350.0),
        ]);

        let mode = analyzer.analyze(&keyframes, 1920, 1080);
        assert_eq!(mode, CameraMode::Tracking);
    }

    #[test]
    fn test_panning_mode() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);

        // Moderate movement (between stationary and tracking thresholds)
        // Need std dev between 0.05 and 0.15, so range ~0.15-0.4 normalized
        // For 1920 width, that's ~300-750 pixels range
        let keyframes = make_keyframes(&[
            (300.0, 300.0, 200.0),
            (500.0, 330.0, 210.0),
            (700.0, 360.0, 220.0),
            (900.0, 390.0, 230.0),
        ]);

        let mode = analyzer.analyze(&keyframes, 1920, 1080);
        assert_eq!(mode, CameraMode::Panning);
    }

    #[test]
    fn test_empty_keyframes() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);
        let mode = analyzer.analyze(&[], 1920, 1080);
        assert_eq!(mode, CameraMode::Stationary);
    }

    #[test]
    fn test_single_keyframe() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);
        let keyframes = make_keyframes(&[(500.0, 300.0, 200.0)]);
        let mode = analyzer.analyze(&keyframes, 1920, 1080);
        assert_eq!(mode, CameraMode::Stationary);
    }

    #[test]
    fn test_motion_stats() {
        let analyzer = CameraModeAnalyzer::with_thresholds(0.05, 0.15);
        let keyframes = make_keyframes(&[
            (200.0, 300.0, 200.0),
            (400.0, 300.0, 200.0),
            (600.0, 300.0, 200.0),
        ]);

        let stats = analyzer.compute_motion_stats(&keyframes, 1920, 1080);
        assert_eq!(stats.keyframe_count, 3);
        assert!(stats.cx_std > 0.0);
        assert!((stats.cy_std - 0.0).abs() < 0.001); // No vertical movement
    }

    #[test]
    fn test_std_dev() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let std = std_dev(&values);
        assert!((std - 2.0).abs() < 0.01); // Expected std dev is 2.0
    }

    #[test]
    fn test_median() {
        assert!((median(&[1.0, 2.0, 3.0]) - 2.0).abs() < 0.001);
        assert!((median(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 0.001);
        assert!((median(&[]) - 0.0).abs() < 0.001);
    }
}
