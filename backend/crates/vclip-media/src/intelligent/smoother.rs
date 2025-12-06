//! Camera path smoothing for jitter-free virtual camera motion.
//!
//! This module provides temporal smoothing of camera positions
//! to create smooth, professional-looking reframing.

use super::config::{FallbackPolicy, IntelligentCropConfig};
use super::models::{BoundingBox, CameraKeyframe, CameraMode, Detection, FrameDetections};
use tracing::debug;

/// Camera smoother for creating smooth camera paths.
pub struct CameraSmoother {
    config: IntelligentCropConfig,
    _fps: f64, // Reserved for future use in timing calculations
}

impl CameraSmoother {
    /// Create a new camera smoother.
    pub fn new(config: IntelligentCropConfig, fps: f64) -> Self {
        Self { config, _fps: fps }
    }

    /// Compute a smooth camera plan from detections.
    ///
    /// # Arguments
    /// * `detections` - Face detections per frame
    /// * `width` - Frame width
    /// * `height` - Frame height
    /// * `start_time` - Start time in seconds
    /// * `end_time` - End time in seconds
    ///
    /// # Returns
    /// Smoothed camera keyframes
    pub fn compute_camera_plan(
        &self,
        detections: &[FrameDetections],
        width: u32,
        height: u32,
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        // Generate raw focus points from detections
        let raw_keyframes = self.compute_raw_focus(detections, width, height, start_time, end_time);

        if raw_keyframes.is_empty() {
            // Fallback to center if no focus points
            return vec![CameraKeyframe::centered(start_time, width, height)];
        }

        // Determine camera mode based on motion
        let mode = self.classify_camera_mode(&raw_keyframes);
        debug!("Camera mode: {:?}", mode);

        // Apply smoothing based on mode
        let smoothed = match mode {
            CameraMode::Static => self.smooth_static(&raw_keyframes),
            CameraMode::Tracking | CameraMode::Zoom => self.smooth_tracking(&raw_keyframes),
        };

        // Enforce motion constraints
        self.enforce_constraints(&smoothed, width, height)
    }

    /// Compute raw focus points from detections.
    fn compute_raw_focus(
        &self,
        detections: &[FrameDetections],
        width: u32,
        height: u32,
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        let sample_interval = 1.0 / self.config.fps_sample;
        let mut keyframes = Vec::new();

        let mut current_time = start_time;
        let mut frame_idx = 0;

        while current_time < end_time && frame_idx < detections.len() {
            let frame_dets = &detections[frame_idx];

            let keyframe = if frame_dets.is_empty() {
                // No detection - use fallback
                self.create_fallback_keyframe(current_time, width, height)
            } else {
                // Compute focus from detections
                let focus = self.compute_focus_from_detections(frame_dets, width, height);
                CameraKeyframe::new(
                    current_time,
                    focus.cx(),
                    focus.cy(),
                    focus.width,
                    focus.height,
                )
            };

            keyframes.push(keyframe);
            current_time += sample_interval;
            frame_idx += 1;
        }

        keyframes
    }

    /// Compute focus region from detections.
    fn compute_focus_from_detections(
        &self,
        detections: &[Detection],
        width: u32,
        height: u32,
    ) -> BoundingBox {
        if detections.is_empty() {
            return self.create_fallback_box(width, height);
        }

        if self.config.prefer_primary_subject && detections.len() > 1 {
            // Use the largest/most confident detection
            let primary = detections
                .iter()
                .max_by(|a, b| {
                    let score_a = a.bbox.area() * a.score;
                    let score_b = b.bbox.area() * b.score;
                    score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();

            let focus_box = primary.bbox.pad(primary.bbox.width * self.config.subject_padding);
            return focus_box.clamp(width, height);
        }

        // Check if faces are close enough to combine
        let faces_far_apart = self.are_faces_far_apart(detections, width);

        if !faces_far_apart {
            // Combine all detections
            let boxes: Vec<BoundingBox> = detections.iter().map(|d| d.bbox).collect();
            if let Some(combined) = BoundingBox::union(&boxes) {
                let focus_box = combined.pad(combined.width * self.config.subject_padding);
                return focus_box.clamp(width, height);
            }
        }

        // Faces far apart - use primary
        let primary = detections
            .iter()
            .max_by(|a, b| {
                let score_a = a.bbox.area() * a.score;
                let score_b = b.bbox.area() * b.score;
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let focus_box = primary.bbox.pad(primary.bbox.width * self.config.subject_padding);
        focus_box.clamp(width, height)
    }

    /// Check if faces are far apart.
    fn are_faces_far_apart(&self, detections: &[Detection], width: u32) -> bool {
        if detections.len() < 2 {
            return false;
        }

        // Compute max distance between face centers
        let mut max_distance: f64 = 0.0;

        for i in 0..detections.len() {
            for j in (i + 1)..detections.len() {
                let dx = detections[i].bbox.cx() - detections[j].bbox.cx();
                let dy = detections[i].bbox.cy() - detections[j].bbox.cy();
                let distance = (dx * dx + dy * dy).sqrt();
                max_distance = max_distance.max(distance);
            }
        }

        // Normalize by frame width
        let normalized = max_distance / width as f64;
        normalized > self.config.multi_face_separation_threshold
    }

    /// Create fallback keyframe based on policy.
    fn create_fallback_keyframe(&self, time: f64, width: u32, height: u32) -> CameraKeyframe {
        let focus = self.create_fallback_box(width, height);
        CameraKeyframe::new(time, focus.cx(), focus.cy(), focus.width, focus.height)
    }

    /// Create fallback bounding box based on policy.
    /// 
    /// For podcast-style videos, faces are typically in the upper 30-45% of frame.
    /// The fallback box should be positioned to capture this region.
    fn create_fallback_box(&self, width: u32, height: u32) -> BoundingBox {
        let w = width as f64;
        let h = height as f64;

        match self.config.fallback_policy {
            FallbackPolicy::Center => {
                // Center of frame - good for general content
                BoundingBox::new(w * 0.2, h * 0.2, w * 0.6, h * 0.6)
            }
            FallbackPolicy::UpperCenter => {
                // Upper-center for talking head / podcast style
                // Position focus box so face center is around 35-40% of frame height
                // Box starts at 15% and is 50% tall, so center is at 40%
                BoundingBox::new(w * 0.15, h * 0.15, w * 0.7, h * 0.5)
            }
            FallbackPolicy::RuleOfThirds => {
                // Rule of thirds - upper third intersection
                BoundingBox::new(w * 0.2, h * 0.15, w * 0.6, h * 0.45)
            }
        }
    }

    /// Classify camera mode based on motion.
    fn classify_camera_mode(&self, keyframes: &[CameraKeyframe]) -> CameraMode {
        if keyframes.len() < 2 {
            return CameraMode::Static;
        }

        // Compute motion statistics
        let cx_values: Vec<f64> = keyframes.iter().map(|kf| kf.cx).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|kf| kf.cy).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|kf| kf.width).collect();

        let cx_std = std_deviation(&cx_values);
        let cy_std = std_deviation(&cy_values);
        let width_std = std_deviation(&width_values);

        let avg_width = mean(&width_values);
        let motion_threshold = avg_width * 0.1;
        let zoom_threshold = avg_width * 0.15;

        if width_std > zoom_threshold {
            CameraMode::Zoom
        } else if cx_std > motion_threshold || cy_std > motion_threshold {
            CameraMode::Tracking
        } else {
            CameraMode::Static
        }
    }

    /// Smooth keyframes for static camera mode.
    fn smooth_static(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.is_empty() {
            return Vec::new();
        }

        // Use median for robustness to outliers
        let cx = median(&keyframes.iter().map(|kf| kf.cx).collect::<Vec<_>>());
        let cy = median(&keyframes.iter().map(|kf| kf.cy).collect::<Vec<_>>());
        let width = median(&keyframes.iter().map(|kf| kf.width).collect::<Vec<_>>());
        let height = median(&keyframes.iter().map(|kf| kf.height).collect::<Vec<_>>());

        keyframes
            .iter()
            .map(|kf| CameraKeyframe::new(kf.time, cx, cy, width, height))
            .collect()
    }

    /// Smooth keyframes for tracking camera mode.
    fn smooth_tracking(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.len() < 3 {
            return keyframes.to_vec();
        }

        // Compute window size in samples
        let duration = keyframes.last().unwrap().time - keyframes.first().unwrap().time;
        let sample_rate = if duration > 0.0 {
            keyframes.len() as f64 / duration
        } else {
            1.0
        };

        let mut window_samples = (self.config.smoothing_window * sample_rate) as usize;
        window_samples = window_samples.max(3);
        if window_samples % 2 == 0 {
            window_samples += 1;
        }

        // Extract arrays
        let cx: Vec<f64> = keyframes.iter().map(|kf| kf.cx).collect();
        let cy: Vec<f64> = keyframes.iter().map(|kf| kf.cy).collect();
        let width: Vec<f64> = keyframes.iter().map(|kf| kf.width).collect();
        let height: Vec<f64> = keyframes.iter().map(|kf| kf.height).collect();

        // Apply moving average
        let cx_smooth = moving_average(&cx, window_samples);
        let cy_smooth = moving_average(&cy, window_samples);
        let width_smooth = moving_average(&width, window_samples);
        let height_smooth = moving_average(&height, window_samples);

        keyframes
            .iter()
            .enumerate()
            .map(|(i, kf)| {
                CameraKeyframe::new(
                    kf.time,
                    cx_smooth[i],
                    cy_smooth[i],
                    width_smooth[i],
                    height_smooth[i],
                )
            })
            .collect()
    }

    /// Enforce motion constraints on keyframes.
    fn enforce_constraints(
        &self,
        keyframes: &[CameraKeyframe],
        width: u32,
        height: u32,
    ) -> Vec<CameraKeyframe> {
        if keyframes.len() < 2 {
            return keyframes.to_vec();
        }

        let mut constrained = Vec::with_capacity(keyframes.len());
        constrained.push(keyframes[0]);

        for i in 1..keyframes.len() {
            let prev = &constrained[i - 1];
            let curr = &keyframes[i];

            let dt = curr.time - prev.time;
            if dt <= 0.0 {
                constrained.push(*curr);
                continue;
            }

            // Compute velocity
            let dx = curr.cx - prev.cx;
            let dy = curr.cy - prev.cy;
            let speed = (dx * dx + dy * dy).sqrt() / dt;

            // Limit speed
            let (new_cx, new_cy) = if speed > self.config.max_pan_speed {
                let scale = self.config.max_pan_speed / speed;
                (prev.cx + dx * scale, prev.cy + dy * scale)
            } else {
                (curr.cx, curr.cy)
            };

            // Clamp to frame bounds
            let margin_x = curr.width / 2.0;
            let margin_y = curr.height / 2.0;
            let clamped_cx = new_cx.max(margin_x).min(width as f64 - margin_x);
            let clamped_cy = new_cy.max(margin_y).min(height as f64 - margin_y);

            constrained.push(CameraKeyframe::new(
                curr.time,
                clamped_cx,
                clamped_cy,
                curr.width,
                curr.height,
            ));
        }

        constrained
    }
}

// === Helper Functions ===

/// Calculate mean of values.
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate standard deviation.
fn std_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Calculate median of values.
fn median(values: &[f64]) -> f64 {
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

/// Apply moving average with edge handling.
fn moving_average(data: &[f64], window: usize) -> Vec<f64> {
    if data.len() < window {
        return data.to_vec();
    }

    let pad = window / 2;
    let mut result = Vec::with_capacity(data.len());

    for i in 0..data.len() {
        let start = if i >= pad { i - pad } else { 0 };
        let end = (i + pad + 1).min(data.len());
        let slice = &data[start..end];
        result.push(slice.iter().sum::<f64>() / slice.len() as f64);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moving_average() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let smoothed = moving_average(&data, 3);

        assert_eq!(smoothed.len(), 5);
        assert!((smoothed[1] - 2.0).abs() < 0.01); // (1+2+3)/3 = 2
        assert!((smoothed[2] - 3.0).abs() < 0.01); // (2+3+4)/3 = 3
    }

    #[test]
    fn test_std_deviation() {
        let data = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let std = std_deviation(&data);
        assert!((std - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_median() {
        let data = vec![1.0, 3.0, 5.0, 7.0, 9.0];
        assert_eq!(median(&data), 5.0);

        let data2 = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(median(&data2), 2.5);
    }

    #[test]
    fn test_camera_mode_classification() {
        let config = IntelligentCropConfig::default();
        let smoother = CameraSmoother::new(config, 30.0);

        // Static - all same position
        let static_kfs = vec![
            CameraKeyframe::new(0.0, 500.0, 500.0, 100.0, 100.0),
            CameraKeyframe::new(1.0, 500.0, 500.0, 100.0, 100.0),
            CameraKeyframe::new(2.0, 500.0, 500.0, 100.0, 100.0),
        ];
        assert_eq!(smoother.classify_camera_mode(&static_kfs), CameraMode::Static);

        // Tracking - position changes
        let tracking_kfs = vec![
            CameraKeyframe::new(0.0, 100.0, 500.0, 100.0, 100.0),
            CameraKeyframe::new(1.0, 300.0, 500.0, 100.0, 100.0),
            CameraKeyframe::new(2.0, 500.0, 500.0, 100.0, 100.0),
        ];
        assert_eq!(smoother.classify_camera_mode(&tracking_kfs), CameraMode::Tracking);
    }
}
