//! Adaptive zoom for cinematic camera motion.
//!
//! This module implements dynamic zoom that adapts based on:
//! - Number of active faces
//! - Face activity levels
//! - Frame composition

use super::config::CinematicConfig;
use crate::intelligent::{BoundingBox, CameraKeyframe, Detection};
use std::collections::HashMap;

/// Adaptive zoom calculator for cinematic framing.
///
/// Implements the user-requested adaptive behavior:
/// - Single active face: Zoom in for tight framing
/// - Multiple active faces: Zoom out to frame all
/// - One dominant speaker: Focus on speaker even if others visible
pub struct AdaptiveZoom {
    /// Minimum zoom level (1.0 = no zoom).
    pub min_zoom: f64,

    /// Maximum zoom level.
    pub max_zoom: f64,

    /// Ideal face ratio relative to frame height (0-1).
    /// 0.25 means face should be ~25% of frame height.
    pub ideal_face_ratio: f64,

    /// Activity threshold to consider a face "active" (0-1).
    pub multi_face_threshold: f64,

    /// Smoothing factor for zoom transitions (0-1).
    /// Lower = faster changes, higher = smoother.
    pub zoom_smoothing: f64,

    /// Frame dimensions for calculations.
    frame_width: f64,
    frame_height: f64,

    /// Previous zoom level for smoothing.
    prev_zoom: Option<f64>,
}

impl AdaptiveZoom {
    /// Create from config with frame dimensions.
    pub fn new(config: &CinematicConfig, frame_width: u32, frame_height: u32) -> Self {
        Self {
            min_zoom: config.min_zoom,
            max_zoom: config.max_zoom,
            ideal_face_ratio: config.ideal_face_ratio,
            multi_face_threshold: config.multi_face_threshold,
            zoom_smoothing: config.zoom_smoothing,
            frame_width: frame_width as f64,
            frame_height: frame_height as f64,
            prev_zoom: None,
        }
    }

    /// Create with explicit parameters.
    pub fn with_params(
        min_zoom: f64,
        max_zoom: f64,
        ideal_face_ratio: f64,
        multi_face_threshold: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> Self {
        Self {
            min_zoom,
            max_zoom,
            ideal_face_ratio,
            multi_face_threshold,
            zoom_smoothing: 0.1,
            frame_width: frame_width as f64,
            frame_height: frame_height as f64,
            prev_zoom: None,
        }
    }

    /// Compute zoom level for a frame with faces and activity scores.
    ///
    /// # Arguments
    /// * `detections` - Face detections for this frame
    /// * `activities` - Activity scores per track_id (0-1)
    ///
    /// # Returns
    /// Recommended zoom level (1.0 = no zoom, higher = more zoom)
    pub fn compute_zoom(
        &mut self,
        detections: &[Detection],
        activities: &HashMap<u32, f64>,
    ) -> f64 {
        if detections.is_empty() {
            return self.apply_smoothing(self.min_zoom);
        }

        // Filter to active faces
        let active_faces: Vec<&Detection> = detections
            .iter()
            .filter(|d| {
                activities.get(&d.track_id).copied().unwrap_or(0.0) > self.multi_face_threshold
            })
            .collect();

        let raw_zoom = match active_faces.len() {
            0 => {
                // No active faces - use largest face as fallback
                if let Some(largest) = self.find_largest_face(detections) {
                    self.compute_single_face_zoom(&largest.bbox)
                } else {
                    self.min_zoom
                }
            }
            1 => {
                // Single active face - tight framing
                self.compute_single_face_zoom(&active_faces[0].bbox)
            }
            _ => {
                // Multiple active faces - frame all
                let boxes: Vec<&BoundingBox> = active_faces.iter().map(|d| &d.bbox).collect();
                self.compute_multi_face_zoom(&boxes)
            }
        };

        self.apply_smoothing(raw_zoom)
    }

    /// Compute zoom for a single face to achieve ideal framing.
    fn compute_single_face_zoom(&self, face: &BoundingBox) -> f64 {
        // Target: face should be ideal_face_ratio of frame height
        let target_face_height = self.frame_height * self.ideal_face_ratio;
        let current_face_height = face.height;

        if current_face_height < 1.0 {
            return self.min_zoom;
        }

        let zoom = target_face_height / current_face_height;
        zoom.clamp(self.min_zoom, self.max_zoom)
    }

    /// Compute zoom to frame multiple faces.
    fn compute_multi_face_zoom(&self, faces: &[&BoundingBox]) -> f64 {
        if faces.is_empty() {
            return self.min_zoom;
        }

        // Compute union bounding box of all faces
        let union = self.compute_union(faces);

        // Target: union should occupy ~70% of frame (with padding)
        let target_height = self.frame_height * 0.7;
        let target_width = self.frame_width * 0.7;

        // Compute zoom needed to fit union
        let zoom_for_height = target_height / union.height.max(1.0);
        let zoom_for_width = target_width / union.width.max(1.0);

        // Use the smaller zoom to ensure everything fits
        let zoom = zoom_for_height.min(zoom_for_width);
        zoom.clamp(self.min_zoom, self.max_zoom)
    }

    /// Compute union bounding box of multiple faces.
    fn compute_union(&self, faces: &[&BoundingBox]) -> BoundingBox {
        if faces.is_empty() {
            return BoundingBox {
                x: 0.0,
                y: 0.0,
                width: self.frame_width,
                height: self.frame_height,
            };
        }

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for face in faces {
            min_x = min_x.min(face.x);
            min_y = min_y.min(face.y);
            max_x = max_x.max(face.x + face.width);
            max_y = max_y.max(face.y + face.height);
        }

        // Add padding (20% on each side)
        let width = max_x - min_x;
        let height = max_y - min_y;
        let padding_x = width * 0.2;
        let padding_y = height * 0.2;

        BoundingBox {
            x: (min_x - padding_x).max(0.0),
            y: (min_y - padding_y).max(0.0),
            width: width + 2.0 * padding_x,
            height: height + 2.0 * padding_y,
        }
    }

    /// Find the largest face by area.
    fn find_largest_face<'a>(&self, detections: &'a [Detection]) -> Option<&'a Detection> {
        detections.iter().max_by(|a, b| {
            let area_a = a.bbox.width * a.bbox.height;
            let area_b = b.bbox.width * b.bbox.height;
            area_a
                .partial_cmp(&area_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Apply temporal smoothing to zoom value.
    fn apply_smoothing(&mut self, target_zoom: f64) -> f64 {
        let smoothed = match self.prev_zoom {
            Some(prev) => {
                // Exponential moving average
                prev + self.zoom_smoothing * (target_zoom - prev)
            }
            None => target_zoom,
        };
        self.prev_zoom = Some(smoothed);
        smoothed
    }

    /// Reset smoothing state (call at scene boundaries).
    pub fn reset(&mut self) {
        self.prev_zoom = None;
    }

    /// Apply adaptive zoom to camera keyframes.
    ///
    /// Modifies the width/height of keyframes based on zoom analysis.
    ///
    /// # Arguments
    /// * `keyframes` - Camera keyframes to modify
    /// * `frame_detections` - Detections per frame (indexed by frame number)
    /// * `activities` - Activity scores per track_id
    /// * `start_time` - Start time of the detection window (seconds)
    /// * `detection_fps` - Sampling rate of `frame_detections` (frames per second)
    pub fn apply_to_keyframes(
        &mut self,
        keyframes: &[CameraKeyframe],
        frame_detections: &[Vec<Detection>],
        activities: &HashMap<u32, f64>,
        start_time: f64,
        detection_fps: f64,
    ) -> Vec<CameraKeyframe> {
        self.reset();

        keyframes
            .iter()
            .map(|kf| {
                // Find corresponding detection frame (frame_detections are sampled at detection_fps)
                let rel_time = (kf.time - start_time).max(0.0);
                let frame_idx = (rel_time * detection_fps).round() as usize;
                let detections = frame_detections
                    .get(frame_idx)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

                // Compute zoom
                let zoom = self.compute_zoom(detections, activities);

                // Apply zoom to keyframe dimensions
                // Smaller width/height = tighter crop = more zoom
                CameraKeyframe {
                    time: kf.time,
                    cx: kf.cx,
                    cy: kf.cy,
                    width: kf.width / zoom,
                    height: kf.height / zoom,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(track_id: u32, x: f64, y: f64, w: f64, h: f64) -> Detection {
        Detection {
            time: 0.0,
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
            score: 0.9,
            track_id,
            mouth_openness: None,
        }
    }

    #[test]
    fn test_single_face_zoom() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);

        // Face is 100px tall, ideal would be 270px (25% of 1080)
        // So zoom should be ~2.7
        let detection = make_detection(0, 500.0, 400.0, 80.0, 100.0);
        let mut activities = HashMap::new();
        activities.insert(0, 0.5); // Active

        let result = zoom.compute_zoom(&[detection], &activities);
        assert!(result > 2.0 && result < 3.0);
    }

    #[test]
    fn test_multi_face_zoom_out() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);

        // Two faces far apart - should zoom out
        let d1 = make_detection(0, 100.0, 300.0, 100.0, 120.0);
        let d2 = make_detection(1, 1700.0, 400.0, 100.0, 120.0);

        let mut activities = HashMap::new();
        activities.insert(0, 0.5);
        activities.insert(1, 0.5);

        let result = zoom.compute_zoom(&[d1, d2], &activities);
        // Should be closer to min zoom since faces are spread out
        assert!(result < 1.5);
    }

    #[test]
    fn test_no_faces_fallback() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);
        let activities = HashMap::new();

        let result = zoom.compute_zoom(&[], &activities);
        assert!((result - 1.0).abs() < 0.1); // Should be min zoom
    }

    #[test]
    fn test_inactive_faces_use_largest() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);

        let d1 = make_detection(0, 100.0, 300.0, 50.0, 60.0);
        let d2 = make_detection(1, 500.0, 300.0, 100.0, 120.0); // Larger

        // No activity scores - both below threshold
        let activities = HashMap::new();

        let result = zoom.compute_zoom(&[d1, d2], &activities);
        // Should use largest face (d2) for zoom calculation
        assert!(result > 1.5); // Zoom in on larger face
    }

    #[test]
    fn test_zoom_smoothing() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);
        zoom.zoom_smoothing = 0.5;

        let d1 = make_detection(0, 500.0, 400.0, 200.0, 240.0);
        let mut activities = HashMap::new();
        activities.insert(0, 0.5);

        // First call - no smoothing
        let _ = zoom.compute_zoom(&[d1.clone()], &activities);

        // Second call with very different face size
        let d2 = make_detection(0, 500.0, 400.0, 50.0, 60.0);
        let z2 = zoom.compute_zoom(&[d2], &activities);

        // With smoothing, z2 should not jump immediately to new value
        let target_zoom = 1080.0 * 0.25 / 60.0; // What it would be without smoothing
        assert!((z2 - target_zoom).abs() > 0.5); // Should be smoothed away from target
    }

    #[test]
    fn test_zoom_clamp() {
        let mut zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);

        // Very small face - would want extreme zoom
        let detection = make_detection(0, 500.0, 400.0, 20.0, 25.0);
        let mut activities = HashMap::new();
        activities.insert(0, 0.5);

        let result = zoom.compute_zoom(&[detection], &activities);
        assert!(result <= 3.0); // Clamped to max

        // Very large face - would want zoom < 1
        zoom.reset();
        let detection = make_detection(0, 100.0, 100.0, 400.0, 480.0);
        let result = zoom.compute_zoom(&[detection], &activities);
        assert!(result >= 1.0); // Clamped to min
    }

    #[test]
    fn test_union_bbox() {
        let zoom = AdaptiveZoom::with_params(1.0, 3.0, 0.25, 0.3, 1920, 1080);

        let b1 = BoundingBox {
            x: 100.0,
            y: 200.0,
            width: 50.0,
            height: 60.0,
        };
        let b2 = BoundingBox {
            x: 300.0,
            y: 250.0,
            width: 50.0,
            height: 60.0,
        };

        let union = zoom.compute_union(&[&b1, &b2]);

        // Union should span from 100 to 350 (x), 200 to 310 (y), plus padding
        assert!(union.x < 100.0); // Padding applied
        assert!(union.y < 200.0);
        assert!(union.width > 250.0);
        assert!(union.height > 110.0);
    }
}
