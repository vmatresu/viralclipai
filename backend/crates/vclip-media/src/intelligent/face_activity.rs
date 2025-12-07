//! Face activity analysis for multi-face scenarios.
//!
//! This module detects which face is active/speaking using visual cues:
//! - Mouth movement detection (via face landmarks)
//! - Motion/frame differencing around faces
//! - Face size and confidence changes
//!
//! Mirrors the Python `FaceActivityAnalyzer` class.

#[cfg(feature = "opencv")]
use opencv::core::Mat;
#[cfg(feature = "opencv")]
use opencv::prelude::MatTraitConst;

use std::collections::HashMap;
use tracing::debug;

use super::face_landmarks::{FaceLandmarkDetector, FaceLandmarks};
use super::models::{BoundingBox, Detection};

/// Configuration for face activity detection.
#[derive(Debug, Clone)]
pub struct FaceActivityConfig {
    /// Enable mouth movement detection (requires LBF landmark model)
    pub enable_mouth_detection: bool,

    /// Time window for aggregating activity scores (seconds)
    pub activity_window: f64,

    /// Minimum duration before switching active face (seconds)
    pub min_switch_duration: f64,

    /// Activity score margin required to switch faces (0.2 = 20% improvement)
    pub switch_margin: f64,

    /// Weight for mouth activity in combined score
    pub weight_mouth: f64,

    /// Weight for motion activity in combined score
    pub weight_motion: f64,

    /// Weight for size changes in combined score
    pub weight_size: f64,

    /// EMA smoothing parameter
    pub smoothing_alpha: f64,
}

impl Default for FaceActivityConfig {
    fn default() -> Self {
        Self {
            enable_mouth_detection: true,
            activity_window: 0.5,
            min_switch_duration: 1.0,
            switch_margin: 0.2,
            weight_mouth: 0.6,
            weight_motion: 0.3,
            weight_size: 0.1,
            smoothing_alpha: 0.3,
        }
    }
}

/// Face activity analyzer that computes per-face activity scores from visual cues.
#[cfg(feature = "opencv")]
pub struct FaceActivityAnalyzer {
    /// Face landmark detector (optional - None if LBF model unavailable)
    landmark_detector: Option<FaceLandmarkDetector>,

    /// Previous frame regions per track ID for motion detection
    prev_frames: HashMap<u32, Mat>,

    /// Face history per track ID: (area, confidence, time)
    face_history: HashMap<u32, Vec<(f64, f64, f64)>>,

    /// Configuration
    config: FaceActivityConfig,
}

#[cfg(feature = "opencv")]
impl FaceActivityAnalyzer {
    /// Create a new face activity analyzer.
    pub fn new(config: FaceActivityConfig) -> Self {
        let landmark_detector = if config.enable_mouth_detection {
            match FaceLandmarkDetector::new() {
                Ok(detector) => detector,
                Err(e) => {
                    debug!("Failed to create landmark detector: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            landmark_detector,
            prev_frames: HashMap::new(),
            face_history: HashMap::new(),
            config,
        }
    }

    /// Compute mouth openness score for a face region.
    ///
    /// Returns `Some(score)` where score is 0.0-1.0, or `None` if landmarks unavailable.
    pub fn compute_mouth_openness(
        &mut self,
        frame: &Mat,
        bbox: &BoundingBox,
    ) -> Option<f64> {
        let detector = self.landmark_detector.as_mut()?;

        match detector.detect_landmarks(frame, bbox) {
            Ok(Some(landmarks)) => Some(landmarks.mouth_openness()),
            Ok(None) => None,
            Err(e) => {
                debug!("Landmark detection error: {}", e);
                None
            }
        }
    }

    /// Compute motion score for a face region using frame differencing.
    ///
    /// Returns a score from 0.0 (no motion) to 1.0 (high motion).
    pub fn compute_motion_score(
        &mut self,
        frame: &Mat,
        bbox: &BoundingBox,
        track_id: u32,
    ) -> f64 {
        use opencv::core::Scalar;
        use opencv::imgproc;

        // Extract current face region
        let curr_region = match self.extract_face_region(frame, bbox) {
            Some(r) => r,
            None => return 0.0,
        };

        // Check for previous frame
        let prev_region = match self.prev_frames.get(&track_id) {
            Some(prev) => prev,
            None => {
                // First frame - store and return 0
                self.prev_frames.insert(track_id, curr_region);
                return 0.0;
            }
        };

        // Ensure same size for comparison
        let prev_resized = if prev_region.size().unwrap_or_default() != curr_region.size().unwrap_or_default() {
            let mut resized = Mat::default();
            if imgproc::resize(
                prev_region,
                &mut resized,
                curr_region.size().unwrap_or_default(),
                0.0,
                0.0,
                imgproc::INTER_LINEAR,
            ).is_err() {
                self.prev_frames.insert(track_id, curr_region);
                return 0.0;
            }
            resized
        } else {
            prev_region.clone()
        };

        // Convert to grayscale for comparison
        let prev_gray = self.to_grayscale(&prev_resized);
        let curr_gray = self.to_grayscale(&curr_region);

        // Compute absolute difference
        let motion_score = match (prev_gray, curr_gray) {
            (Some(pg), Some(cg)) => {
                let mut diff = Mat::default();
                if opencv::core::absdiff(&pg, &cg, &mut diff).is_ok() {
                    // Calculate mean of difference
                    let mean = opencv::core::mean(&diff, &Mat::default())
                        .unwrap_or(Scalar::all(0.0));
                    
                    // Normalize: typical values are 0-30 for normal motion
                    let raw_score = mean.0[0] / 255.0;
                    (raw_score * 10.0).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };

        // Update stored frame
        self.prev_frames.insert(track_id, curr_region);

        motion_score
    }

    /// Convert Mat to grayscale.
    fn to_grayscale(&self, mat: &Mat) -> Option<Mat> {
        use opencv::imgproc;

        let channels = mat.channels();
        if channels == 1 {
            return Some(mat.clone());
        }

        let mut gray = Mat::default();
        let code = if channels == 3 {
            imgproc::COLOR_BGR2GRAY
        } else if channels == 4 {
            imgproc::COLOR_BGRA2GRAY
        } else {
            return None;
        };

        match imgproc::cvt_color(mat, &mut gray, code, 0, opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT) {
            Ok(_) => Some(gray),
            Err(_) => None,
        }
    }

    /// Extract face region from frame.
    fn extract_face_region(&self, frame: &Mat, bbox: &BoundingBox) -> Option<Mat> {
        use opencv::core::Rect;

        if frame.empty() {
            return None;
        }

        let frame_w = frame.cols();
        let frame_h = frame.rows();

        // Clamp bbox to frame bounds
        let x = (bbox.x.max(0.0) as i32).min(frame_w - 1);
        let y = (bbox.y.max(0.0) as i32).min(frame_h - 1);
        let w = (bbox.width as i32).min(frame_w - x).max(1);
        let h = (bbox.height as i32).min(frame_h - y).max(1);

        let rect = Rect::new(x, y, w, h);

        match Mat::roi(frame, rect) {
            Ok(roi) => {
                // Clone to own the data
                let mut owned = Mat::default();
                if roi.copy_to(&mut owned).is_ok() {
                    Some(owned)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    /// Compute score based on face size and confidence changes.
    ///
    /// Faces that are growing or have increasing confidence may be becoming
    /// more prominent (speaking, moving forward).
    pub fn compute_size_change_score(
        &mut self,
        bbox: &BoundingBox,
        score: f64,
        track_id: u32,
        time: f64,
    ) -> f64 {
        // Store current state
        let face_area = bbox.area();
        let history = self.face_history.entry(track_id).or_default();
        history.push((face_area, score, time));

        // Keep only recent history
        let window_start = time - self.config.activity_window;
        history.retain(|(_, _, t)| *t >= window_start);

        if history.len() < 2 {
            return 0.0;
        }

        // Compute trend
        let first = history.first().unwrap();
        let last = history.last().unwrap();

        // Area trend (normalized)
        let area_trend = (last.0 - first.0) / (first.0 + 1e-6);

        // Confidence trend
        let conf_trend = last.1 - first.1;

        // Combined score (positive = growing/prominent)
        let size_score = area_trend * 0.7 + conf_trend * 0.3;

        // Normalize to 0-1 (we care about increases)
        size_score.clamp(-1.0, 1.0).max(0.0)
    }

    /// Compute overall activity score for a face detection.
    ///
    /// Combines mouth movement, motion, and size changes with configured weights.
    pub fn compute_activity_score(
        &mut self,
        frame: &Mat,
        detection: &Detection,
    ) -> f64 {
        let mut scores: Vec<f64> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();

        // Mouth movement
        if self.config.weight_mouth > 0.0 && self.landmark_detector.is_some() {
            if let Some(mouth_score) = self.compute_mouth_openness(frame, &detection.bbox) {
                scores.push(mouth_score);
                weights.push(self.config.weight_mouth);
            }
        }

        // Motion
        if self.config.weight_motion > 0.0 {
            let motion_score = self.compute_motion_score(frame, &detection.bbox, detection.track_id);
            scores.push(motion_score);
            weights.push(self.config.weight_motion);
        }

        // Size change
        if self.config.weight_size > 0.0 {
            let size_score = self.compute_size_change_score(
                &detection.bbox,
                detection.score,
                detection.track_id,
                detection.time,
            );
            scores.push(size_score);
            weights.push(self.config.weight_size);
        }

        if scores.is_empty() {
            return 0.0;
        }

        // Weighted average
        let total_weight: f64 = weights.iter().sum();
        if total_weight == 0.0 {
            return 0.0;
        }

        let activity: f64 = scores.iter().zip(weights.iter())
            .map(|(s, w)| s * w)
            .sum::<f64>() / total_weight;

        activity.clamp(0.0, 1.0)
    }

    /// Clean up resources for a track that's no longer active.
    pub fn cleanup_track(&mut self, track_id: u32) {
        self.prev_frames.remove(&track_id);
        self.face_history.remove(&track_id);
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.prev_frames.clear();
        self.face_history.clear();
    }

    /// Get detected landmarks for a face (for debugging).
    pub fn get_landmarks(
        &mut self,
        frame: &Mat,
        bbox: &BoundingBox,
    ) -> Option<FaceLandmarks> {
        let detector = self.landmark_detector.as_mut()?;
        detector.detect_landmarks(frame, bbox).ok().flatten()
    }
}

/// Stub for when OpenCV is not available
#[cfg(not(feature = "opencv"))]
pub struct FaceActivityAnalyzer {
    config: FaceActivityConfig,
}

#[cfg(not(feature = "opencv"))]
impl FaceActivityAnalyzer {
    pub fn new(config: FaceActivityConfig) -> Self {
        Self { config }
    }

    pub fn compute_activity_score(&mut self, _detection: &Detection) -> f64 {
        0.0
    }

    pub fn cleanup_track(&mut self, _track_id: u32) {}

    pub fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = FaceActivityConfig::default();
        assert!(config.enable_mouth_detection);
        assert_eq!(config.activity_window, 0.5);
        assert_eq!(config.min_switch_duration, 1.0);
        assert!((config.weight_mouth + config.weight_motion + config.weight_size - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_size_change_tracking() {
        let config = FaceActivityConfig::default();
        
        #[cfg(feature = "opencv")]
        {
            let mut analyzer = FaceActivityAnalyzer::new(config);
            
            // First detection - returns 0.0 (not enough history)
            let bbox1 = BoundingBox::new(100.0, 100.0, 50.0, 50.0);
            let score1 = analyzer.compute_size_change_score(&bbox1, 0.8, 1, 0.0);
            assert_eq!(score1, 0.0);
            
            // Second detection with larger size - should show positive trend
            let bbox2 = BoundingBox::new(100.0, 100.0, 60.0, 60.0);
            let score2 = analyzer.compute_size_change_score(&bbox2, 0.85, 1, 0.1);
            assert!(score2 > 0.0, "Growing face should have positive size score: {}", score2);
        }
    }

    #[test]
    fn test_cleanup_track() {
        let config = FaceActivityConfig::default();
        
        #[cfg(feature = "opencv")]
        {
            let mut analyzer = FaceActivityAnalyzer::new(config);
            
            // Add some history
            let bbox = BoundingBox::new(100.0, 100.0, 50.0, 50.0);
            analyzer.compute_size_change_score(&bbox, 0.8, 1, 0.0);
            analyzer.compute_size_change_score(&bbox, 0.8, 1, 0.1);
            
            assert!(analyzer.face_history.contains_key(&1));
            
            // Cleanup
            analyzer.cleanup_track(1);
            
            assert!(!analyzer.face_history.contains_key(&1));
        }
    }
}
