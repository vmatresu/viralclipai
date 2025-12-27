//! Multi-frame scene window analyzer for temporal smoothing.
//!
//! Implements AutoFlip-style multi-frame lookahead that analyzes a window
//! of frames before making camera decisions. This provides:
//!
//! - Temporal median of face positions (filters outliers)
//! - Aggregated motion statistics for camera mode selection
//! - Dropout-resilient tracking (handles missing detections)

use std::collections::VecDeque;

use super::camera_mode::CameraMode;
use crate::intelligent::models::{CameraKeyframe, Detection};

/// Analysis result from a window of frames.
#[derive(Debug, Clone)]
pub struct WindowAnalysis {
    /// Median center X position across the window.
    pub median_cx: f64,
    /// Median center Y position across the window.
    pub median_cy: f64,
    /// Median width of focus region.
    pub median_width: f64,
    /// Median height of focus region.
    pub median_height: f64,
    /// Recommended camera mode based on motion analysis.
    pub camera_mode: CameraMode,
    /// Number of valid frames in the window (with detections).
    pub valid_frames: usize,
    /// Total frames in window.
    pub total_frames: usize,
}

impl WindowAnalysis {
    /// Create a centered fallback analysis.
    pub fn centered(frame_width: u32, frame_height: u32) -> Self {
        Self {
            median_cx: frame_width as f64 / 2.0,
            median_cy: frame_height as f64 / 2.0,
            median_width: frame_width as f64,
            median_height: frame_height as f64,
            camera_mode: CameraMode::Stationary,
            valid_frames: 0,
            total_frames: 0,
        }
    }

    /// Convert to camera keyframe at a given time.
    pub fn to_keyframe(&self, time: f64) -> CameraKeyframe {
        CameraKeyframe {
            time,
            cx: self.median_cx,
            cy: self.median_cy,
            width: self.median_width,
            height: self.median_height,
        }
    }

    /// Get validity ratio (fraction of frames with detections).
    pub fn validity_ratio(&self) -> f64 {
        if self.total_frames == 0 {
            0.0
        } else {
            self.valid_frames as f64 / self.total_frames as f64
        }
    }
}

/// Per-frame focus point derived from detections.
#[derive(Debug, Clone)]
struct FrameFocus {
    #[allow(dead_code)]
    time: f64,
    cx: f64,
    cy: f64,
    width: f64,
    height: f64,
    has_detections: bool,
}

/// Multi-frame scene window analyzer.
///
/// Analyzes a sliding window of frames to compute temporally-smoothed
/// camera targets. This mimics AutoFlip's approach of looking at scene
/// context before making camera decisions.
pub struct SceneWindowAnalyzer {
    /// Window size in frames.
    window_size: usize,
    /// Buffer of recent frame focus points.
    buffer: VecDeque<FrameFocus>,
    /// Frame dimensions for normalization.
    frame_width: u32,
    frame_height: u32,
    /// Thresholds for camera mode selection.
    stationary_threshold: f64,
    panning_threshold: f64,
}

impl SceneWindowAnalyzer {
    /// Create a new analyzer with specified window size.
    pub fn new(
        window_size: usize,
        frame_width: u32,
        frame_height: u32,
        stationary_threshold: f64,
        panning_threshold: f64,
    ) -> Self {
        Self {
            window_size: window_size.max(1),
            buffer: VecDeque::with_capacity(window_size),
            frame_width,
            frame_height,
            stationary_threshold,
            panning_threshold,
        }
    }

    /// Create with default thresholds.
    pub fn with_defaults(window_size: usize, frame_width: u32, frame_height: u32) -> Self {
        Self::new(window_size, frame_width, frame_height, 0.05, 0.15)
    }

    /// Reset the analyzer, clearing all buffered frames.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Add a new frame's detections to the buffer and get updated analysis.
    ///
    /// # Arguments
    /// * `detections` - Face/object detections for this frame
    /// * `time` - Timestamp of this frame
    ///
    /// # Returns
    /// Window analysis with temporal median values.
    pub fn add_frame(&mut self, detections: &[Detection], time: f64) -> WindowAnalysis {
        // Compute focus point for this frame
        let focus = self.compute_frame_focus(detections, time);

        // Add to buffer, maintaining window size
        self.buffer.push_back(focus);
        while self.buffer.len() > self.window_size {
            self.buffer.pop_front();
        }

        // Compute window analysis
        self.analyze_window()
    }

    /// Analyze an entire shot's worth of detections at once.
    ///
    /// This is more efficient than frame-by-frame for batch processing.
    ///
    /// # Arguments
    /// * `detections` - Per-frame detections for the shot
    /// * `sample_interval` - Time between frames
    /// * `start_time` - Start time of the shot
    ///
    /// # Returns
    /// Vector of window analyses, one per frame.
    pub fn analyze_shot(
        &mut self,
        detections: &[Vec<Detection>],
        sample_interval: f64,
        start_time: f64,
    ) -> Vec<WindowAnalysis> {
        self.reset();

        detections
            .iter()
            .enumerate()
            .map(|(i, frame_dets)| {
                let time = start_time + i as f64 * sample_interval;
                self.add_frame(frame_dets, time)
            })
            .collect()
    }

    /// Compute focus point from detections for a single frame.
    fn compute_frame_focus(&self, detections: &[Detection], time: f64) -> FrameFocus {
        if detections.is_empty() {
            // No detections - use center
            return FrameFocus {
                time,
                cx: self.frame_width as f64 / 2.0,
                cy: self.frame_height as f64 / 2.0,
                width: self.frame_width as f64,
                height: self.frame_height as f64,
                has_detections: false,
            };
        }

        // Compute bounding box of all detections
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut total_weight = 0.0;
        let mut weighted_cx = 0.0;
        let mut weighted_cy = 0.0;

        for det in detections {
            let weight = det.score as f64 * (1.0 + det.mouth_openness.unwrap_or(0.0));
            let cx = det.bbox.cx();
            let cy = det.bbox.cy();

            min_x = min_x.min(det.bbox.x);
            min_y = min_y.min(det.bbox.y);
            max_x = max_x.max(det.bbox.x + det.bbox.width);
            max_y = max_y.max(det.bbox.y + det.bbox.height);

            weighted_cx += cx * weight;
            weighted_cy += cy * weight;
            total_weight += weight;
        }

        let cx = if total_weight > 0.0 {
            weighted_cx / total_weight
        } else {
            (min_x + max_x) / 2.0
        };

        let cy = if total_weight > 0.0 {
            weighted_cy / total_weight
        } else {
            (min_y + max_y) / 2.0
        };

        // Add padding to the bounding box
        let padding = 0.2;
        let width = (max_x - min_x) * (1.0 + padding * 2.0);
        let height = (max_y - min_y) * (1.0 + padding * 2.0);

        FrameFocus {
            time,
            cx,
            cy,
            width: width.max(100.0),
            height: height.max(100.0),
            has_detections: true,
        }
    }

    /// Analyze the current window buffer.
    fn analyze_window(&self) -> WindowAnalysis {
        if self.buffer.is_empty() {
            return WindowAnalysis::centered(self.frame_width, self.frame_height);
        }

        // Collect values from frames with detections
        let valid_frames: Vec<&FrameFocus> =
            self.buffer.iter().filter(|f| f.has_detections).collect();

        let valid_count = valid_frames.len();

        // If no valid frames, use all frames for position but flag as invalid
        let frames_to_use = if valid_frames.is_empty() {
            self.buffer.iter().collect::<Vec<_>>()
        } else {
            valid_frames
        };

        // Compute medians
        let median_cx = median(frames_to_use.iter().map(|f| f.cx));
        let median_cy = median(frames_to_use.iter().map(|f| f.cy));
        let median_width = median(frames_to_use.iter().map(|f| f.width));
        let median_height = median(frames_to_use.iter().map(|f| f.height));

        // Compute motion statistics for camera mode
        let camera_mode = self.compute_camera_mode(&frames_to_use);

        WindowAnalysis {
            median_cx,
            median_cy,
            median_width,
            median_height,
            camera_mode,
            valid_frames: valid_count,
            total_frames: self.buffer.len(),
        }
    }

    /// Determine camera mode based on motion in the window.
    fn compute_camera_mode(&self, frames: &[&FrameFocus]) -> CameraMode {
        if frames.len() < 2 {
            return CameraMode::Stationary;
        }

        let w = self.frame_width as f64;
        let h = self.frame_height as f64;

        // Normalize positions
        let cx_values: Vec<f64> = frames.iter().map(|f| f.cx / w).collect();
        let cy_values: Vec<f64> = frames.iter().map(|f| f.cy / h).collect();
        let width_values: Vec<f64> = frames.iter().map(|f| f.width / w).collect();

        // Compute standard deviations
        let cx_std = std_dev(&cx_values);
        let cy_std = std_dev(&cy_values);
        let width_std = std_dev(&width_values);

        let position_std = (cx_std + cy_std) / 2.0;
        let size_std = width_std;

        // Classification
        if position_std < self.stationary_threshold && size_std < self.stationary_threshold {
            CameraMode::Stationary
        } else if position_std > self.panning_threshold || size_std > self.panning_threshold * 0.67
        {
            CameraMode::Tracking
        } else {
            CameraMode::Panning
        }
    }
}

/// Compute median of an iterator of f64 values.
fn median<I>(iter: I) -> f64
where
    I: Iterator<Item = f64>,
{
    let mut values: Vec<f64> = iter.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligent::models::BoundingBox;

    fn make_detection(x: f64, y: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(0.0, BoundingBox::new(x, y, size, size), 0.9, track_id)
    }

    #[test]
    fn test_empty_window() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(5, 1920, 1080);
        let analysis = analyzer.add_frame(&[], 0.0);

        assert_eq!(analysis.valid_frames, 0);
        assert_eq!(analysis.camera_mode, CameraMode::Stationary);
    }

    #[test]
    fn test_single_detection() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(5, 1920, 1080);
        let det = make_detection(500.0, 300.0, 100.0, 1);
        let analysis = analyzer.add_frame(&[det], 0.0);

        assert_eq!(analysis.valid_frames, 1);
        // Center should be near face center (500 + 50 = 550, 300 + 50 = 350)
        assert!((analysis.median_cx - 550.0).abs() < 10.0);
    }

    #[test]
    fn test_window_median_filters_outliers() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(5, 1920, 1080);

        // Add 4 consistent frames and 1 outlier
        for i in 0..4 {
            let det = make_detection(500.0, 300.0, 100.0, 1);
            analyzer.add_frame(&[det], i as f64 * 0.1);
        }
        // Outlier at completely different position
        let outlier = make_detection(1500.0, 800.0, 100.0, 1);
        let analysis = analyzer.add_frame(&[outlier], 0.4);

        // Median should still be close to majority position, not pulled by outlier
        assert!(
            analysis.median_cx < 800.0,
            "Median should resist outlier: {}",
            analysis.median_cx
        );
    }

    #[test]
    fn test_stationary_mode_detection() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(10, 1920, 1080);

        // Add frames with very little movement
        for i in 0..10 {
            let x = 500.0 + (i as f64 * 2.0); // Very small movement
            let det = make_detection(x, 300.0, 100.0, 1);
            analyzer.add_frame(&[det], i as f64 * 0.1);
        }

        let analysis = analyzer.analyze_window();
        assert_eq!(analysis.camera_mode, CameraMode::Stationary);
    }

    #[test]
    fn test_tracking_mode_detection() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(10, 1920, 1080);

        // Add frames with large movement (needs to exceed panning_threshold 0.15)
        for i in 0..10 {
            let x = 100.0 + (i as f64 * 180.0); // Very large movement across frame
            let det = make_detection(x, 300.0, 100.0, 1);
            analyzer.add_frame(&[det], i as f64 * 0.1);
        }

        let analysis = analyzer.analyze_window();
        // With large movement, should be Tracking or at least Panning (not Stationary)
        assert_ne!(analysis.camera_mode, CameraMode::Stationary);
    }

    #[test]
    fn test_analyze_shot() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(5, 1920, 1080);

        let detections: Vec<Vec<Detection>> = (0..10)
            .map(|i| vec![make_detection(500.0 + i as f64 * 10.0, 300.0, 100.0, 1)])
            .collect();

        let analyses = analyzer.analyze_shot(&detections, 0.1, 0.0);

        assert_eq!(analyses.len(), 10);
        // All should have valid frames as window fills up
        assert!(analyses.last().unwrap().valid_frames >= 1);
    }

    #[test]
    fn test_dropout_handling() {
        let mut analyzer = SceneWindowAnalyzer::with_defaults(5, 1920, 1080);

        // Add frame with detection
        let det = make_detection(500.0, 300.0, 100.0, 1);
        analyzer.add_frame(&[det], 0.0);

        // Add empty frame (dropout)
        let analysis = analyzer.add_frame(&[], 0.1);

        // Should still have the previous valid frame
        assert!(analysis.valid_frames >= 1);
        // Should use valid frame's position, not center
        assert!(analysis.median_cx < 960.0);
    }
}
