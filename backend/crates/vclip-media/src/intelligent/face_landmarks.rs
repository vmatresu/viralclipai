//! Face landmark detection using OpenCV's LBF (Local Binary Features) facemark model.
//!
//! Provides 68-point facial landmarks for detecting mouth openness and other facial expressions.
//! This is an optional component - falls back gracefully if LBF model is not available.
//!
//! # Landmarks Layout (LBF 68-point model)
//!
//! - 0-16: Jaw outline
//! - 17-21: Right eyebrow
//! - 22-26: Left eyebrow
//! - 27-35: Nose
//! - 36-41: Right eye
//! - 42-47: Left eye
//! - 48-59: Outer lip
//! - 60-67: Inner lip
//!
//! # Note
//! This module requires OpenCV's contrib face module which is not currently
//! enabled in the build. When landmarks are unavailable, face activity detection
//! falls back to motion and size-based scoring.

use super::models::BoundingBox;
use crate::error::MediaResult;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{debug, warn};

/// Global LBF model availability flag
static LBF_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// LBF model paths in priority order
const LBF_MODEL_PATHS: &[&str] = &[
    // Backend models directory
    "/app/backend/models/face_detection/lbfmodel.yaml",
    "/app/models/face_detection/lbfmodel.yaml",
    "/app/models/lbfmodel.yaml",
    // Relative paths for development
    "./backend/models/face_detection/lbfmodel.yaml",
    // System paths
    "/usr/share/opencv/testdata/cv/face/lbfmodel.yaml",
];

/// Check if LBF model is available
pub fn is_lbf_available() -> bool {
    *LBF_AVAILABLE.get_or_init(|| {
        // Currently disabled - face contrib module not in build
        // When enabled: check for model file
        if find_lbf_model_path().is_some() {
            // Model file exists, but opencv::face module may not be enabled
            debug!("LBF model file found but face contrib module not enabled in build");
            false
        } else {
            debug!("LBF model not found - mouth detection disabled");
            false
        }
    })
}

/// Find LBF model path
fn find_lbf_model_path() -> Option<&'static str> {
    for path in LBF_MODEL_PATHS {
        if Path::new(path).exists() {
            return Some(path);
        }
    }
    None
}

/// Indices for lip landmarks in the 68-point model
pub const OUTER_LIP_UPPER: &[usize] = &[48, 49, 50, 51, 52, 53, 54];
pub const OUTER_LIP_LOWER: &[usize] = &[54, 55, 56, 57, 58, 59, 48];
pub const INNER_LIP_UPPER: &[usize] = &[60, 61, 62, 63, 64];
pub const INNER_LIP_LOWER: &[usize] = &[64, 65, 66, 67, 60];

/// 68-point facial landmarks from LBF model.
#[derive(Debug, Clone)]
pub struct FaceLandmarks {
    /// 68 landmark coordinates in (x, y) format
    pub points: Vec<(f64, f64)>,
    /// Confidence score (if available)
    pub confidence: f64,
}

impl FaceLandmarks {
    /// Create landmarks from a vector of points.
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self {
            points,
            confidence: 1.0,
        }
    }

    /// Calculate mouth openness score (0.0 = closed, 1.0 = wide open).
    ///
    /// Uses the distance between upper and lower inner lip landmarks,
    /// normalized by the face height (jaw to eyebrow distance).
    pub fn mouth_openness(&self) -> f64 {
        if self.points.len() < 68 {
            return 0.0;
        }

        // Get inner lip landmark averages
        let upper_lip_y: f64 = INNER_LIP_UPPER
            .iter()
            .filter_map(|&i| self.points.get(i))
            .map(|(_, y)| *y)
            .sum::<f64>()
            / INNER_LIP_UPPER.len() as f64;

        let lower_lip_y: f64 = INNER_LIP_LOWER
            .iter()
            .filter_map(|&i| self.points.get(i))
            .map(|(_, y)| *y)
            .sum::<f64>()
            / INNER_LIP_LOWER.len() as f64;

        // Mouth opening distance
        let mouth_height = (lower_lip_y - upper_lip_y).abs();

        // Normalize by face height (point 8 is chin, point 27 is between eyebrows)
        let chin_y = self.points.get(8).map(|(_, y)| *y).unwrap_or(0.0);
        let brow_y = self.points.get(27).map(|(_, y)| *y).unwrap_or(0.0);
        let face_height = (chin_y - brow_y).abs();

        if face_height < 10.0 {
            return 0.0;
        }

        // Typical mouth opening ranges from 0.02 to 0.15 of face height
        let normalized = mouth_height / face_height;

        // Scale to 0-1 range (closed=0.02, wide open=0.12)
        ((normalized - 0.02) / 0.10).clamp(0.0, 1.0)
    }

    /// Get mouth center position.
    pub fn mouth_center(&self) -> (f64, f64) {
        if self.points.len() < 68 {
            return (0.0, 0.0);
        }

        // Average of all inner lip points (indices 60-67)
        let inner_lip: Vec<_> = (60..68).filter_map(|i| self.points.get(i)).collect();
        if inner_lip.is_empty() {
            return (0.0, 0.0);
        }

        let sum_x: f64 = inner_lip.iter().map(|(x, _)| *x).sum();
        let sum_y: f64 = inner_lip.iter().map(|(_, y)| *y).sum();
        let count = inner_lip.len() as f64;

        (sum_x / count, sum_y / count)
    }

    /// Get face bounding box from landmarks.
    pub fn bounding_box(&self) -> BoundingBox {
        if self.points.is_empty() {
            return BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        }

        let min_x = self.points.iter().map(|(x, _)| *x).fold(f64::MAX, f64::min);
        let max_x = self.points.iter().map(|(x, _)| *x).fold(f64::MIN, f64::max);
        let min_y = self.points.iter().map(|(_, y)| *y).fold(f64::MAX, f64::min);
        let max_y = self.points.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);

        BoundingBox::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// Face landmark detector placeholder.
///
/// Currently returns None for all detections because OpenCV's contrib `face` module
/// is not enabled in the build. When/if face contrib is added to the Cargo.toml features,
/// this can be implemented properly.
///
/// To enable face landmarks in the future, add to opencv features in Cargo.toml:
/// ```toml
/// opencv = { ..., features = [..., "face"] }
/// ```
/// And rebuild with OpenCV contrib installed.
pub struct FaceLandmarkDetector {
    _private: (),
}

impl FaceLandmarkDetector {
    /// Create a new face landmark detector.
    ///
    /// Currently always returns `Ok(None)` because the face contrib module is not enabled.
    pub fn new() -> MediaResult<Option<Self>> {
        // Face contrib module not in build - log once and return None
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            warn!(
                "Face landmark detection disabled: OpenCV face contrib module not in build. \
                 Face activity will use motion and size metrics only."
            );
        });
        Ok(None)
    }

    /// Detect landmarks for a face region.
    ///
    /// Always returns `Ok(None)` because face contrib is not enabled.
    #[cfg(feature = "opencv")]
    pub fn detect_landmarks(
        &mut self,
        _frame: &opencv::core::Mat,
        _face_bbox: &BoundingBox,
    ) -> MediaResult<Option<FaceLandmarks>> {
        Ok(None)
    }

    #[cfg(not(feature = "opencv"))]
    pub fn detect_landmarks(
        &mut self,
        _frame: &(),
        _face_bbox: &BoundingBox,
    ) -> MediaResult<Option<FaceLandmarks>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_landmark_indices() {
        // Verify lip indices are valid for 68-point model
        assert!(OUTER_LIP_UPPER.iter().all(|&i| i < 68));
        assert!(OUTER_LIP_LOWER.iter().all(|&i| i < 68));
        assert!(INNER_LIP_UPPER.iter().all(|&i| i < 68));
        assert!(INNER_LIP_LOWER.iter().all(|&i| i < 68));
    }

    #[test]
    fn test_mouth_openness_closed() {
        // Simulate closed mouth (upper and lower lip at same position)
        let mut points = vec![(100.0, 100.0); 68];
        
        // Set chin and brow for face height normalization
        points[8] = (100.0, 200.0); // chin
        points[27] = (100.0, 50.0); // brow
        
        // Set inner lip points close together
        for i in 60..68 {
            points[i] = (100.0, 125.0);
        }
        
        let landmarks = FaceLandmarks::new(points);
        let openness = landmarks.mouth_openness();
        
        assert!(openness < 0.2, "Closed mouth should have low openness: {}", openness);
    }

    #[test]
    fn test_mouth_openness_open() {
        let mut points = vec![(100.0, 100.0); 68];
        
        // Set chin and brow for face height normalization
        points[8] = (100.0, 200.0); // chin
        points[27] = (100.0, 50.0); // brow (face height = 150)
        
        // Set inner lip points far apart (upper at 120, lower at 140)
        // This is ~13% of face height
        for &i in INNER_LIP_UPPER {
            points[i] = (100.0, 120.0);
        }
        for &i in INNER_LIP_LOWER {
            if i != 60 {  // avoid overlap
                points[i] = (100.0, 140.0);
            }
        }
        
        let landmarks = FaceLandmarks::new(points);
        let openness = landmarks.mouth_openness();
        
        assert!(openness > 0.5, "Open mouth should have high openness: {}", openness);
    }

    #[test]
    fn test_mouth_center() {
        let mut points = vec![(0.0, 0.0); 68];
        
        // Place inner lip points around center (100, 150)
        for i in 60..68 {
            points[i] = (100.0 + (i as f64 - 63.5) * 10.0, 150.0);
        }
        
        let landmarks = FaceLandmarks::new(points);
        let (cx, cy) = landmarks.mouth_center();
        
        assert!((cx - 100.0).abs() < 1.0, "Center X should be ~100: {}", cx);
        assert!((cy - 150.0).abs() < 1.0, "Center Y should be ~150: {}", cy);
    }

    #[test]
    fn test_bounding_box() {
        let points = vec![
            (10.0, 20.0),
            (50.0, 30.0),
            (30.0, 80.0),
        ];
        
        let landmarks = FaceLandmarks {
            points,
            confidence: 1.0,
        };
        
        let bbox = landmarks.bounding_box();
        assert_eq!(bbox.x, 10.0);
        assert_eq!(bbox.y, 20.0);
        assert_eq!(bbox.width, 40.0);
        assert_eq!(bbox.height, 60.0);
    }

    #[test]
    fn test_detector_returns_none() {
        // Detector should return None when face contrib not available
        let detector = FaceLandmarkDetector::new();
        assert!(detector.unwrap().is_none());
    }
}
