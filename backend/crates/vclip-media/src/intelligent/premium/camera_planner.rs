//! Premium camera planner for intelligent_speaker style.
//!
//! Orchestrates target selection, smoothing, and crop computation
//! for the highest-tier Active Speaker mode.

use super::config::PremiumSpeakerConfig;
use super::crop_computer::{CropComputer, CropComputeConfig};
use super::smoothing::PremiumSmoother;
use super::target_selector::CameraTargetSelector;
use crate::intelligent::models::{AspectRatio, CameraKeyframe, CropWindow, FrameDetections};

/// Premium camera planner for the intelligent_speaker style.
pub struct PremiumCameraPlanner {
    config: PremiumSpeakerConfig,
    target_selector: CameraTargetSelector,
    smoother: PremiumSmoother,
    crop_computer: CropComputer,
    frame_width: u32,
    frame_height: u32,
}

impl PremiumCameraPlanner {
    /// Create a new premium camera planner.
    pub fn new(
        config: PremiumSpeakerConfig,
        frame_width: u32,
        frame_height: u32,
        fps: f64,
    ) -> Self {
        let crop_config = CropComputeConfig::from(&config);

        Self {
            target_selector: CameraTargetSelector::new(config.clone(), frame_width, frame_height),
            smoother: PremiumSmoother::new(config.clone(), fps, frame_width, frame_height),
            crop_computer: CropComputer::new(crop_config, frame_width, frame_height),
            config,
            frame_width,
            frame_height,
        }
    }

    /// Compute camera plan from detections.
    pub fn compute_camera_plan(
        &mut self,
        detections: &[FrameDetections],
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        if detections.is_empty() {
            return vec![self.fallback_keyframe(start_time)];
        }

        let sample_interval = (end_time - start_time) / detections.len().max(1) as f64;
        let mut keyframes = Vec::with_capacity(detections.len());
        let mut prev_track_ids: Vec<u32> = Vec::new();

        for (i, frame_dets) in detections.iter().enumerate() {
            let time = start_time + i as f64 * sample_interval;
            let current_ids: Vec<u32> = frame_dets.iter().map(|d| d.track_id).collect();

            if self.is_scene_change(&prev_track_ids, &current_ids) {
                let focus = self.target_selector.select_focus(frame_dets, time);
                self.smoother.soft_reset(&focus, time);
            }

            prev_track_ids = current_ids;

            let focus = self.target_selector.select_focus(frame_dets, time);
            let smoothed = self.smoother.smooth(&focus, time);
            keyframes.push(smoothed);
        }

        keyframes
    }

    /// Compute crop windows from camera keyframes.
    pub fn compute_crop_windows(
        &self,
        keyframes: &[CameraKeyframe],
        target_aspect: &AspectRatio,
    ) -> Vec<CropWindow> {
        self.crop_computer.compute_crop_windows(keyframes, target_aspect)
    }

    fn is_scene_change(&self, prev_ids: &[u32], current_ids: &[u32]) -> bool {
        if !self.config.enable_scene_detection || prev_ids.is_empty() {
            return false;
        }

        let common_count = current_ids.iter()
            .filter(|id| prev_ids.contains(id))
            .count();

        let total = prev_ids.len().max(current_ids.len());
        if total > 0 {
            let overlap_ratio = common_count as f64 / total as f64;
            return overlap_ratio < (1.0 - self.config.scene_change_threshold);
        }

        false
    }

    fn fallback_keyframe(&self, time: f64) -> CameraKeyframe {
        let w = self.frame_width as f64;
        let h = self.frame_height as f64;
        CameraKeyframe::new(time, w / 2.0, h * 0.4, w * 0.6, h * 0.5)
    }

    /// Reset camera state.
    pub fn reset(&mut self) {
        self.smoother.reset();
    }

    /// Get current primary subject.
    pub fn current_primary_subject(&self) -> Option<u32> {
        self.target_selector.current_primary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligent::models::{BoundingBox, Detection};

    fn make_detections(positions: &[(f64, f64, u32)], time: f64) -> FrameDetections {
        positions
            .iter()
            .map(|(x, y, id)| {
                Detection::new(time, BoundingBox::new(*x, *y, 200.0, 200.0), 0.9, *id)
            })
            .collect()
    }

    #[test]
    fn test_camera_plan_generation() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            make_detections(&[(500.0, 400.0, 1)], 0.0),
            make_detections(&[(600.0, 400.0, 1)], 0.1),
            make_detections(&[(700.0, 400.0, 1)], 0.2),
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.3);
        assert_eq!(keyframes.len(), 3);

        for i in 1..keyframes.len() {
            let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
            assert!(dx < 200.0, "Motion not smooth: {}", dx);
        }
    }

    #[test]
    fn test_crop_window_generation() {
        let config = PremiumSpeakerConfig::default();
        let planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let keyframes = vec![CameraKeyframe::new(0.0, 960.0, 540.0, 200.0, 300.0)];
        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        assert_eq!(crops.len(), 1);
        let ratio = crops[0].width as f64 / crops[0].height as f64;
        assert!((ratio - 0.5625).abs() < 0.02);
    }

    #[test]
    fn test_scene_change_detection() {
        let mut config = PremiumSpeakerConfig::default();
        config.enable_scene_detection = true;
        config.scene_change_threshold = 0.5;

        let planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        assert!(!planner.is_scene_change(&[1, 2], &[1, 2]));
        assert!(planner.is_scene_change(&[1, 2], &[3, 4]));
    }

    #[test]
    fn test_empty_detections_fallback() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let keyframes = planner.compute_camera_plan(&[], 0.0, 1.0);
        assert_eq!(keyframes.len(), 1);
        assert!(keyframes[0].cx > 0.0);
    }
}
