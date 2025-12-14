//! Face saliency signal extraction from detections.
//!
//! Converts face detections into weighted saliency signals using
//! the `SignalFusingCalculator` with configurable weights.

use super::super::signal_fusion::{SaliencySignal, SignalFusingCalculator};
use crate::detection::{ObjectDetection, COCO_CLASSES};
use crate::intelligent::models::{BoundingBox, Detection};

/// Per-frame saliency signals.
#[derive(Debug, Clone)]
pub struct PerFrameSaliency {
    /// Timestamp in seconds
    pub time: f64,
    /// Saliency signals for this frame
    pub signals: Vec<SaliencySignal>,
}

impl PerFrameSaliency {
    /// Create a new per-frame saliency container.
    pub fn new(time: f64, signals: Vec<SaliencySignal>) -> Self {
        Self { time, signals }
    }

    /// Check if this frame has any signals.
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    /// Get the number of signals.
    pub fn len(&self) -> usize {
        self.signals.len()
    }
}

/// Face signal extraction from detections.
///
/// Converts face detections into weighted saliency signals suitable for
/// camera targeting and focus point computation.
pub struct FaceSignals {
    /// Signal fusion calculator with configured weights
    fusion: SignalFusingCalculator,
}

impl FaceSignals {
    /// Create with default weights.
    pub fn new() -> Self {
        Self {
            fusion: SignalFusingCalculator::new(),
        }
    }

    /// Create with custom weights.
    pub fn with_weights(face_weight: f64, activity_boost: f64) -> Self {
        Self {
            fusion: SignalFusingCalculator::new()
                .with_face_weight(face_weight)
                .with_activity_boost(activity_boost),
        }
    }

    /// Get a reference to the internal fusion calculator.
    pub fn fusion(&self) -> &SignalFusingCalculator {
        &self.fusion
    }

    /// Convert detections to per-frame saliency signals.
    ///
    /// # Arguments
    /// * `detections` - Per-frame face detections
    /// * `sample_interval` - Time between detection samples (1/fps)
    /// * `start_time` - Start time offset
    ///
    /// # Returns
    /// Per-frame saliency signals with weighted face importance.
    pub fn from_detections(
        &self,
        detections: &[Vec<Detection>],
        sample_interval: f64,
        start_time: f64,
    ) -> Vec<PerFrameSaliency> {
        detections
            .iter()
            .enumerate()
            .map(|(i, frame_dets)| {
                let time = start_time + i as f64 * sample_interval;
                let signals = self.fusion.fuse_faces(frame_dets);
                PerFrameSaliency::new(time, signals)
            })
            .collect()
    }

    /// Compute fused saliency from faces and objects.
    ///
    /// Combines face detections with object detections (e.g., YOLOv8 results)
    /// into a unified set of saliency signals. Person objects get a 1.5x weight
    /// boost since they often represent important subjects.
    ///
    /// # Arguments
    /// * `faces` - Face detections for this frame
    /// * `objects` - Object detections from YOLO/similar
    /// * `object_weight` - Base weight for object signals (default ~0.2)
    ///
    /// # Returns
    /// Combined saliency signals from all sources.
    pub fn compute_fused_saliency(
        &self,
        faces: &[Detection],
        objects: &[ObjectDetection],
        object_weight: f64,
    ) -> Vec<SaliencySignal> {
        let mut signals = self.fusion.fuse_faces(faces);

        for obj in objects {
            // Person objects get boosted weight
            let weight = if obj.is_person() {
                object_weight * 1.5
            } else {
                object_weight
            };

            // Convert normalized coordinates to pixel coordinates
            // (SaliencySignal uses pixel coordinates)
            let class_name = if obj.class_id < COCO_CLASSES.len() {
                COCO_CLASSES[obj.class_id].to_string()
            } else {
                format!("class_{}", obj.class_id)
            };

            signals.push(SaliencySignal::from_object(
                BoundingBox {
                    x: obj.x as f64,
                    y: obj.y as f64,
                    width: obj.width as f64,
                    height: obj.height as f64,
                },
                obj.class_id as u32,
                class_name,
                weight * obj.confidence as f64,
                obj.is_person(),
            ));
        }

        signals
    }

    /// Compute weighted focus point for a single frame.
    ///
    /// # Returns
    /// (cx, cy) weighted center coordinate, or None if no signals.
    pub fn compute_focus_for_frame(&self, frame_dets: &[Detection]) -> Option<(f64, f64)> {
        if frame_dets.is_empty() {
            return None;
        }

        let signals = self.fusion.fuse_faces(frame_dets);
        if signals.is_empty() {
            return None;
        }

        Some(self.fusion.compute_focus_point(&signals))
    }

    /// Compute focus bounding box for a single frame.
    ///
    /// # Arguments
    /// * `frame_dets` - Detections for this frame
    /// * `frame_width` - Video width
    /// * `frame_height` - Video height
    /// * `padding` - Padding fraction around required signals
    ///
    /// # Returns
    /// Bounding box for camera focus, or centered fallback if no signals.
    pub fn compute_focus_bounds(
        &self,
        frame_dets: &[Detection],
        frame_width: u32,
        frame_height: u32,
        padding: f64,
    ) -> BoundingBox {
        let signals = self.fusion.fuse_faces(frame_dets);
        self.fusion
            .compute_combined_focus(&signals, frame_width, frame_height, padding)
    }
}

impl Default for FaceSignals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligent::models::BoundingBox;

    fn make_detection(
        track_id: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        mouth: Option<f64>,
    ) -> Detection {
        Detection {
            time: 0.0,
            bbox: BoundingBox { x, y, width: w, height: h },
            score: 0.9,
            track_id,
            mouth_openness: mouth,
        }
    }

    #[test]
    fn test_face_signals_from_detections() {
        let signals = FaceSignals::new();
        let detections = vec![
            vec![make_detection(1, 100.0, 100.0, 50.0, 50.0, Some(0.5))],
            vec![make_detection(1, 110.0, 100.0, 50.0, 50.0, Some(0.3))],
        ];

        let saliency = signals.from_detections(&detections, 0.1, 0.0);
        assert_eq!(saliency.len(), 2);
        assert!(!saliency[0].is_empty());
        assert_eq!(saliency[0].len(), 1);
    }

    #[test]
    fn test_compute_focus_for_frame() {
        let signals = FaceSignals::new();
        let dets = vec![make_detection(1, 100.0, 100.0, 100.0, 100.0, None)];

        let focus = signals.compute_focus_for_frame(&dets);
        assert!(focus.is_some());

        let (cx, _cy) = focus.unwrap();
        // Focus should be at face center: 100 + 50 = 150
        assert!((cx - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_empty_frame_no_focus() {
        let signals = FaceSignals::new();
        let focus = signals.compute_focus_for_frame(&[]);
        assert!(focus.is_none());
    }

    #[test]
    fn test_custom_weights() {
        let signals = FaceSignals::with_weights(2.0, 1.0);
        assert!((signals.fusion().face_weight - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_focus_bounds_fallback() {
        let signals = FaceSignals::new();
        let bounds = signals.compute_focus_bounds(&[], 1920, 1080, 0.2);
        
        // Should get centered fallback
        assert!(bounds.cx() > 400.0 && bounds.cx() < 1500.0);
    }
}
