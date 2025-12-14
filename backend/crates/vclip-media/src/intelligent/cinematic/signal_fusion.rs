//! Signal fusion for saliency-weighted camera targeting.
//!
//! Implements AutoAI-style signal fusion that combines multiple saliency signals
//! (faces, objects, safe regions) into a weighted focus point for camera tracking.
//!
//! # AutoAI Signal Fusion
//!
//! AutoAI combines multiple detection signals with configurable weights:
//! - **Faces**: Highest priority (weight ~1.0), always try to keep visible
//! - **People**: Medium priority (weight ~0.5), important but secondary
//! - **Objects**: Lower priority (weight ~0.2), contextual importance
//! - **Safe Regions**: Borders/logos to avoid cropping
//!
//! The fusion algorithm computes a weighted center of all signals and determines
//! the minimum bounding box that keeps all "required" signals in frame.

use crate::intelligent::models::{BoundingBox, Detection};

/// Source of a saliency signal.
#[derive(Debug, Clone)]
pub enum SignalSource {
    /// Face detection with track ID and activity score.
    Face {
        track_id: u32,
        activity: f64, // Mouth openness or visual activity (0-1)
    },
    /// Object detection with COCO class info.
    Object {
        class_id: u32,
        class_name: String,
    },
    /// Safe region that should not be cropped (logo, border, text).
    SafeRegion {
        description: String,
    },
}

impl SignalSource {
    /// Check if this signal is from face detection.
    pub fn is_face(&self) -> bool {
        matches!(self, SignalSource::Face { .. })
    }

    /// Check if this signal is a "person" object detection.
    pub fn is_person(&self) -> bool {
        matches!(self, SignalSource::Object { class_id: 0, .. })
    }
}

/// A saliency signal from a detector.
#[derive(Debug, Clone)]
pub struct SaliencySignal {
    /// Bounding box of the salient region.
    pub bbox: BoundingBox,

    /// Importance weight (0-1). Higher = more important for camera targeting.
    pub weight: f64,

    /// If true, this region MUST be kept visible in the crop.
    /// If false, it's optional and may be cropped if needed.
    pub is_required: bool,

    /// Source of this signal (face, object, safe region).
    pub source: SignalSource,
}

impl SaliencySignal {
    /// Create a signal from a face detection.
    pub fn from_face(detection: &Detection, weight: f64, is_required: bool) -> Self {
        let activity = detection.mouth_openness.unwrap_or(0.0);
        Self {
            bbox: detection.bbox.clone(),
            weight,
            is_required,
            source: SignalSource::Face {
                track_id: detection.track_id,
                activity,
            },
        }
    }

    /// Create a signal from an object detection.
    pub fn from_object(
        bbox: BoundingBox,
        class_id: u32,
        class_name: String,
        weight: f64,
        is_required: bool,
    ) -> Self {
        Self {
            bbox,
            weight,
            is_required,
            source: SignalSource::Object { class_id, class_name },
        }
    }

    /// Create a signal for a safe region.
    pub fn from_safe_region(bbox: BoundingBox, description: String) -> Self {
        Self {
            bbox,
            weight: 0.0, // Safe regions don't affect center, only avoid cropping
            is_required: true,
            source: SignalSource::SafeRegion { description },
        }
    }

    /// Weighted center X coordinate.
    pub fn weighted_cx(&self) -> f64 {
        self.bbox.cx() * self.weight
    }

    /// Weighted center Y coordinate.
    pub fn weighted_cy(&self) -> f64 {
        self.bbox.cy() * self.weight
    }
}

/// Fuses multiple saliency signals into unified camera targets.
///
/// This implements the AutoAI SignalFusingCalculator pattern:
/// 1. Collects signals from multiple detectors
/// 2. Assigns weights based on signal type and configuration
/// 3. Computes weighted focus point
/// 4. Ensures required signals are kept visible
pub struct SignalFusingCalculator {
    /// Weight for face signals (default: 1.0).
    pub face_weight: f64,

    /// Weight for "person" object signals (default: 0.5).
    pub person_weight: f64,

    /// Weight for other object signals (default: 0.2).
    pub object_weight: f64,

    /// Activity boost factor - faces with high mouth activity get boosted weight.
    pub activity_boost: f64,

    /// Whether faces are always required (must be visible).
    pub faces_required: bool,
}

impl Default for SignalFusingCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalFusingCalculator {
    /// Create with default weights.
    pub fn new() -> Self {
        Self {
            face_weight: 1.0,
            person_weight: 0.5,
            object_weight: 0.2,
            activity_boost: 0.5,
            faces_required: true,
        }
    }

    /// Set custom face weight.
    pub fn with_face_weight(mut self, weight: f64) -> Self {
        self.face_weight = weight;
        self
    }

    /// Set custom person weight.
    pub fn with_person_weight(mut self, weight: f64) -> Self {
        self.person_weight = weight;
        self
    }

    /// Set custom object weight.
    pub fn with_object_weight(mut self, weight: f64) -> Self {
        self.object_weight = weight;
        self
    }

    /// Set activity boost factor.
    pub fn with_activity_boost(mut self, boost: f64) -> Self {
        self.activity_boost = boost;
        self
    }

    /// Fuse face detections into saliency signals.
    ///
    /// Each face gets:
    /// - Base weight from `face_weight`
    /// - Activity boost if mouth is open (speaking)
    pub fn fuse_faces(&self, detections: &[Detection]) -> Vec<SaliencySignal> {
        detections
            .iter()
            .map(|det| {
                let activity = det.mouth_openness.unwrap_or(0.0);
                let weight = self.face_weight + activity * self.activity_boost;
                SaliencySignal::from_face(det, weight, self.faces_required)
            })
            .collect()
    }

    /// Compute the weighted focus point from all signals.
    ///
    /// Returns (cx, cy) weighted center coordinate.
    pub fn compute_focus_point(&self, signals: &[SaliencySignal]) -> (f64, f64) {
        if signals.is_empty() {
            return (0.0, 0.0);
        }

        // Skip signals with zero weight (safe regions don't affect center)
        let weighted_signals: Vec<_> = signals.iter().filter(|s| s.weight > 0.0).collect();

        if weighted_signals.is_empty() {
            // Fallback to unweighted center
            let cx: f64 = signals.iter().map(|s| s.bbox.cx()).sum::<f64>() / signals.len() as f64;
            let cy: f64 = signals.iter().map(|s| s.bbox.cy()).sum::<f64>() / signals.len() as f64;
            return (cx, cy);
        }

        let total_weight: f64 = weighted_signals.iter().map(|s| s.weight).sum();
        let cx = weighted_signals.iter().map(|s| s.weighted_cx()).sum::<f64>() / total_weight;
        let cy = weighted_signals.iter().map(|s| s.weighted_cy()).sum::<f64>() / total_weight;

        (cx, cy)
    }

    /// Compute the minimum bounding box that contains all required signals.
    ///
    /// This ensures that faces and other required elements stay in frame.
    pub fn compute_required_bounds(&self, signals: &[SaliencySignal]) -> Option<BoundingBox> {
        let required: Vec<_> = signals.iter().filter(|s| s.is_required).collect();

        if required.is_empty() {
            return None;
        }

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for signal in required {
            min_x = min_x.min(signal.bbox.x);
            min_y = min_y.min(signal.bbox.y);
            max_x = max_x.max(signal.bbox.x + signal.bbox.width);
            max_y = max_y.max(signal.bbox.y + signal.bbox.height);
        }

        Some(BoundingBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        })
    }

    /// Compute combined focus bounding box for camera targeting.
    ///
    /// The result:
    /// - Center is the weighted average of all signal centers
    /// - Size ensures all required signals fit within the crop
    ///
    /// # Arguments
    /// * `signals` - All saliency signals for this frame
    /// * `frame_width` - Source video width
    /// * `frame_height` - Source video height
    /// * `padding` - Extra padding around required bounds (fraction, e.g., 0.2)
    pub fn compute_combined_focus(
        &self,
        signals: &[SaliencySignal],
        frame_width: u32,
        frame_height: u32,
        padding: f64,
    ) -> BoundingBox {
        let frame_w = frame_width as f64;
        let frame_h = frame_height as f64;

        if signals.is_empty() {
            // Fallback to center frame
            return BoundingBox {
                x: frame_w * 0.25,
                y: frame_h * 0.25,
                width: frame_w * 0.5,
                height: frame_h * 0.5,
            };
        }

        // Get weighted focus point
        let (focus_cx, focus_cy) = self.compute_focus_point(signals);

        // Get required bounds
        let required_bounds = self.compute_required_bounds(signals);

        // Compute result size based on required bounds + padding
        let (width, height) = if let Some(bounds) = required_bounds {
            let pad_w = bounds.width * padding;
            let pad_h = bounds.height * padding;
            (bounds.width + 2.0 * pad_w, bounds.height + 2.0 * pad_h)
        } else {
            // No required signals - use default size
            (frame_w * 0.5, frame_h * 0.5)
        };

        // Create bounding box centered on focus point
        let x = (focus_cx - width / 2.0).max(0.0).min(frame_w - width);
        let y = (focus_cy - height / 2.0).max(0.0).min(frame_h - height);

        BoundingBox {
            x,
            y,
            width: width.min(frame_w),
            height: height.min(frame_h),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(track_id: u32, x: f64, y: f64, w: f64, h: f64, mouth: Option<f64>) -> Detection {
        Detection {
            time: 0.0,
            bbox: BoundingBox { x, y, width: w, height: h },
            score: 0.9,
            track_id,
            mouth_openness: mouth,
        }
    }

    #[test]
    fn test_face_signal_creation() {
        let det = make_detection(1, 100.0, 200.0, 50.0, 60.0, Some(0.5));
        let signal = SaliencySignal::from_face(&det, 1.0, true);

        assert!(signal.source.is_face());
        assert!(signal.is_required);
        assert!((signal.weight - 1.0).abs() < 0.001);
        assert_eq!(signal.bbox.x, 100.0);
    }

    #[test]
    fn test_object_signal_creation() {
        let bbox = BoundingBox { x: 100.0, y: 200.0, width: 50.0, height: 60.0 };
        let signal = SaliencySignal::from_object(bbox, 0, "person".to_string(), 0.5, false);

        assert!(signal.source.is_person());
        assert!(!signal.is_required);
        assert!((signal.weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_safe_region_signal() {
        let bbox = BoundingBox { x: 0.0, y: 0.0, width: 100.0, height: 50.0 };
        let signal = SaliencySignal::from_safe_region(bbox, "logo".to_string());

        assert!(signal.is_required);
        assert!((signal.weight - 0.0).abs() < 0.001); // Safe regions have 0 weight
    }

    #[test]
    fn test_fuse_faces_adds_activity_boost() {
        let fusioner = SignalFusingCalculator::new();

        let det_quiet = make_detection(1, 100.0, 200.0, 50.0, 60.0, Some(0.0));
        let det_speaking = make_detection(2, 500.0, 200.0, 50.0, 60.0, Some(0.8));

        let signals = fusioner.fuse_faces(&[det_quiet, det_speaking]);

        assert_eq!(signals.len(), 2);
        // Speaking face should have higher weight
        assert!(signals[1].weight > signals[0].weight);
    }

    #[test]
    fn test_focus_point_weighted_center() {
        let fusioner = SignalFusingCalculator::new();

        let det1 = make_detection(1, 100.0, 200.0, 100.0, 100.0, None);
        let det2 = make_detection(2, 500.0, 200.0, 100.0, 100.0, Some(1.0));

        let signals = fusioner.fuse_faces(&[det1, det2]);
        let (cx, _cy) = fusioner.compute_focus_point(&signals);

        // Speaking face (det2) has higher weight, so center should be closer to it
        // det1 center: 150, det2 center: 550
        // With activity boost, det2 weight > det1 weight
        assert!(cx > 300.0, "Focus should be pulled toward speaking face");
    }

    #[test]
    fn test_required_bounds() {
        let fusioner = SignalFusingCalculator::new();

        let det1 = make_detection(1, 100.0, 200.0, 50.0, 60.0, None);
        let det2 = make_detection(2, 500.0, 300.0, 80.0, 100.0, None);

        let signals = fusioner.fuse_faces(&[det1, det2]);
        let bounds = fusioner.compute_required_bounds(&signals).unwrap();

        // Should span from (100, 200) to (580, 400)
        assert!((bounds.x - 100.0).abs() < 0.001);
        assert!((bounds.y - 200.0).abs() < 0.001);
        assert!((bounds.width - 480.0).abs() < 0.001); // 580 - 100
        assert!((bounds.height - 200.0).abs() < 0.001); // 400 - 200
    }

    #[test]
    fn test_combined_focus_empty() {
        let fusioner = SignalFusingCalculator::new();

        let result = fusioner.compute_combined_focus(&[], 1920, 1080, 0.2);

        // Should fall back to center
        assert!(result.cx() > 400.0 && result.cx() < 1500.0);
    }

    #[test]
    fn test_combined_focus_single_face() {
        let fusioner = SignalFusingCalculator::new();

        let det = make_detection(1, 800.0, 400.0, 100.0, 120.0, None);
        let signals = fusioner.fuse_faces(&[det]);

        let result = fusioner.compute_combined_focus(&signals, 1920, 1080, 0.2);

        // Focus should be centered on the face
        let face_cx = 800.0 + 50.0; // 850
        assert!((result.cx() - face_cx).abs() < 50.0, "Focus should be near face center");
    }

    #[test]
    fn test_face_weight_customization() {
        let fusioner = SignalFusingCalculator::new().with_face_weight(2.0);
        assert!((fusioner.face_weight - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_no_required_signals() {
        let fusioner = SignalFusingCalculator::new();

        // Create signal that is not required
        let signal = SaliencySignal {
            bbox: BoundingBox { x: 100.0, y: 200.0, width: 50.0, height: 60.0 },
            weight: 0.5,
            is_required: false,
            source: SignalSource::Object {
                class_id: 1,
                class_name: "dog".to_string(),
            },
        };

        let bounds = fusioner.compute_required_bounds(&[signal]);
        assert!(bounds.is_none());
    }
}
