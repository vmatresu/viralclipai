//! Premium camera planner for intelligent_speaker style.
//!
//! Orchestrates target selection, smoothing, and crop computation
//! for the highest-tier Active Speaker mode.
//!
//! Key improvements:
//! - Uses real timestamps from detections instead of synthetic uniform timing
//! - Robust handling of detection dropouts
//! - Strong scene change detection and adaptation
//! - All processing is purely visual (NO audio)

use tracing::debug;

use super::config::PremiumSpeakerConfig;
use super::crop_computer::{CropComputeConfig, CropComputer};
use super::smoothing::PremiumSmoother;
use super::target_selector::CameraTargetSelector;
use crate::intelligent::models::{AspectRatio, CameraKeyframe, CropWindow, FrameDetections};

/// Debug statistics for camera planning.
#[derive(Debug, Default)]
pub struct PlannerStats {
    pub total_frames: usize,
    pub frames_with_detections: usize,
    pub dropout_frames: usize,
    pub scene_changes: usize,
    pub subject_switches: usize,
    pub max_pan_speed: f64,
    pub max_zoom: f64,
    pub min_zoom: f64,
}

/// Premium camera planner for the intelligent_speaker style.
pub struct PremiumCameraPlanner {
    config: PremiumSpeakerConfig,
    target_selector: CameraTargetSelector,
    smoother: PremiumSmoother,
    crop_computer: CropComputer,
    frame_width: u32,
    frame_height: u32,
    fps: f64,
    /// Statistics for debugging
    stats: PlannerStats,
    /// Last primary subject for tracking switches
    last_primary: Option<u32>,
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
            fps,
            stats: PlannerStats::default(),
            last_primary: None,
        }
    }

    /// Compute camera plan from detections using real timestamps.
    ///
    /// Instead of synthetic uniform timing, this extracts actual timestamps
    /// from detection data for accurate dt calculations.
    pub fn compute_camera_plan(
        &mut self,
        detections: &[FrameDetections],
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        if detections.is_empty() {
            return vec![self.fallback_keyframe(start_time)];
        }

        self.stats = PlannerStats::default();
        self.stats.total_frames = detections.len();
        self.stats.min_zoom = f64::MAX;
        self.stats.max_zoom = 0.0;

        let mut keyframes: Vec<CameraKeyframe> = Vec::with_capacity(detections.len());
        let mut prev_track_ids: Vec<u32> = Vec::new();
        let mut last_known_time = start_time;
        let mut last_known_dt = (end_time - start_time) / detections.len().max(1) as f64;

        for (i, frame_dets) in detections.iter().enumerate() {
            // Extract real timestamp from detections
            let time = self.extract_frame_time(
                frame_dets,
                i,
                start_time,
                end_time,
                &mut last_known_time,
                &mut last_known_dt,
            );

            // Check for scene change
            let current_ids: Vec<u32> = frame_dets.iter().map(|d| d.track_id).collect();
            let is_scene_change = self.is_scene_change(&prev_track_ids, &current_ids);

            if is_scene_change {
                self.stats.scene_changes += 1;
                if self.config.enable_debug_logging {
                    debug!("Scene change at frame {} (t={:.2}s)", i, time);
                }
            }

            // Handle scene change with soft reset
            if is_scene_change && !frame_dets.is_empty() {
                let focus = self.target_selector.select_focus(frame_dets, time);
                self.smoother.soft_reset(&focus, time);
            }

            prev_track_ids = current_ids;

            // Select focus and smooth
            let focus = self.target_selector.select_focus(frame_dets, time);

            // Track subject switches
            if let Some(last) = self.last_primary {
                if focus.track_id != last && focus.track_id != 0 {
                    self.stats.subject_switches += 1;
                }
            }
            self.last_primary = Some(focus.track_id);

            // Track detection stats
            if frame_dets.is_empty() {
                self.stats.dropout_frames += 1;
            } else {
                self.stats.frames_with_detections += 1;
            }

            let smoothed = self.smoother.smooth(&focus, time);

            // Track zoom stats
            let zoom = self.frame_width as f64 / smoothed.width;
            self.stats.max_zoom = self.stats.max_zoom.max(zoom);
            self.stats.min_zoom = self.stats.min_zoom.min(zoom);

            // Track pan speed
            if let Some(prev_kf) = keyframes.last() {
                let dt = smoothed.time - prev_kf.time;
                if dt > 0.0 {
                    let dx = smoothed.cx - prev_kf.cx;
                    let dy = smoothed.cy - prev_kf.cy;
                    let speed = (dx * dx + dy * dy).sqrt() / dt;
                    self.stats.max_pan_speed = self.stats.max_pan_speed.max(speed);
                }
            }

            keyframes.push(smoothed);
        }

        if self.config.enable_debug_logging {
            debug!(
                "Camera plan: {} frames, {} detections, {} dropouts, {} scene changes, {} switches",
                self.stats.total_frames,
                self.stats.frames_with_detections,
                self.stats.dropout_frames,
                self.stats.scene_changes,
                self.stats.subject_switches
            );
            debug!(
                "  Max pan speed: {:.0} px/s, Zoom range: {:.2} - {:.2}",
                self.stats.max_pan_speed, self.stats.min_zoom, self.stats.max_zoom
            );
        }

        keyframes
    }

    /// Extract real timestamp from frame detections.
    /// Falls back to interpolation if no detections have timestamps.
    fn extract_frame_time(
        &self,
        frame_dets: &FrameDetections,
        frame_idx: usize,
        start_time: f64,
        end_time: f64,
        last_known_time: &mut f64,
        last_known_dt: &mut f64,
    ) -> f64 {
        // Try to get time from detections
        if !frame_dets.is_empty() {
            // Use median time from detections for robustness
            let mut times: Vec<f64> = frame_dets.iter().map(|d| d.time).collect();
            times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let median_time = if times.len() % 2 == 0 {
                (times[times.len() / 2 - 1] + times[times.len() / 2]) / 2.0
            } else {
                times[times.len() / 2]
            };

            // Validate the time is reasonable
            if median_time >= start_time && median_time <= end_time {
                let dt = median_time - *last_known_time;
                if dt > 0.0 {
                    *last_known_dt = dt;
                }
                *last_known_time = median_time;
                return median_time;
            }
        }

        // Fallback: advance by last known dt or estimate from fps
        let estimated_dt = if *last_known_dt > 0.0 {
            *last_known_dt
        } else {
            1.0 / self.fps.max(1.0)
        };

        let time = if frame_idx == 0 {
            start_time
        } else {
            (*last_known_time + estimated_dt).min(end_time)
        };

        *last_known_time = time;
        time
    }

    /// Compute crop windows from camera keyframes.
    pub fn compute_crop_windows(
        &self,
        keyframes: &[CameraKeyframe],
        target_aspect: &AspectRatio,
    ) -> Vec<CropWindow> {
        self.crop_computer
            .compute_crop_windows(keyframes, target_aspect)
    }

    /// Detect scene change based on track ID continuity.
    fn is_scene_change(&self, prev_ids: &[u32], current_ids: &[u32]) -> bool {
        if !self.config.enable_scene_detection || prev_ids.is_empty() {
            return false;
        }

        // Significant change in detection count
        let count_diff = (current_ids.len() as i32 - prev_ids.len() as i32).abs();
        if count_diff >= 2 {
            return true;
        }

        // Check track ID overlap
        let common_count = current_ids
            .iter()
            .filter(|id| prev_ids.contains(id))
            .count();

        let total = prev_ids.len().max(current_ids.len());
        if total > 0 {
            let overlap_ratio = common_count as f64 / total as f64;
            return overlap_ratio < (1.0 - self.config.scene_change_threshold);
        }

        false
    }

    /// Generate fallback keyframe for empty detection sequences.
    fn fallback_keyframe(&self, time: f64) -> CameraKeyframe {
        let w = self.frame_width as f64;
        let h = self.frame_height as f64;
        CameraKeyframe::new(time, w / 2.0, h * 0.4, w * 0.6, h * 0.5)
    }

    /// Reset camera state.
    pub fn reset(&mut self) {
        self.smoother.reset();
        self.target_selector.reset_primary_subject();
        self.last_primary = None;
        self.stats = PlannerStats::default();
    }

    /// Get current primary subject.
    pub fn current_primary_subject(&self) -> Option<u32> {
        self.target_selector.current_primary()
    }

    /// Get planning statistics.
    pub fn stats(&self) -> &PlannerStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligent::models::{BoundingBox, Detection};

    fn make_detection(time: f64, x: f64, y: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(time, BoundingBox::new(x, y, size, size), 0.9, track_id)
    }

    fn make_detections(positions: &[(f64, f64, u32)], time: f64) -> FrameDetections {
        positions
            .iter()
            .map(|(x, y, id)| make_detection(time, *x, *y, 200.0, *id))
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
    fn test_real_timestamps_used() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        // Detections with specific timestamps
        let detections = vec![
            make_detections(&[(500.0, 400.0, 1)], 0.0),
            make_detections(&[(600.0, 400.0, 1)], 0.5), // 0.5s gap
            make_detections(&[(700.0, 400.0, 1)], 0.6), // 0.1s gap
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 1.0);

        // Keyframes should use real timestamps
        assert!((keyframes[0].time - 0.0).abs() < 0.01);
        assert!((keyframes[1].time - 0.5).abs() < 0.01);
        assert!((keyframes[2].time - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_dropout_handling() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            make_detections(&[(500.0, 400.0, 1)], 0.0),
            vec![], // Dropout
            vec![], // Dropout
            make_detections(&[(550.0, 400.0, 1)], 0.3),
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.4);
        assert_eq!(keyframes.len(), 4);

        // During dropout, position should be held
        let dx_dropout = (keyframes[2].cx - keyframes[1].cx).abs();
        assert!(dx_dropout < 50.0, "Should hold position during dropout");

        assert_eq!(planner.stats().dropout_frames, 2);
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
    fn test_scene_change_triggers_soft_reset() {
        let mut config = PremiumSpeakerConfig::default();
        config.enable_scene_detection = true;

        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            make_detections(&[(200.0, 400.0, 1)], 0.0),
            make_detections(&[(200.0, 400.0, 1)], 0.1),
            make_detections(&[(1500.0, 400.0, 10)], 0.2), // Scene change - new track
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.3);

        // After scene change, camera should move toward new subject faster
        let dx = (keyframes[2].cx - keyframes[1].cx).abs();
        assert!(dx > 100.0, "Scene change should allow faster repositioning");

        assert!(planner.stats().scene_changes >= 1);
    }

    #[test]
    fn test_empty_detections_fallback() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let keyframes = planner.compute_camera_plan(&[], 0.0, 1.0);
        assert_eq!(keyframes.len(), 1);
        assert!(keyframes[0].cx > 0.0);
    }

    #[test]
    fn test_stats_tracking() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            make_detections(&[(500.0, 400.0, 1)], 0.0),
            vec![],
            make_detections(&[(600.0, 400.0, 1)], 0.2),
        ];

        planner.compute_camera_plan(&detections, 0.0, 0.3);

        let stats = planner.stats();
        assert_eq!(stats.total_frames, 3);
        assert_eq!(stats.frames_with_detections, 2);
        assert_eq!(stats.dropout_frames, 1);
    }

    #[test]
    fn test_subject_switch_tracking() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        // Simulate subject switch after dwell time
        let mut detections = Vec::new();
        for i in 0..20 {
            let time = i as f64 * 0.1;
            let (size1, size2) = if i < 15 {
                (300.0, 150.0)
            } else {
                (150.0, 350.0)
            };
            detections.push(vec![
                Detection::new(time, BoundingBox::new(200.0, 400.0, size1, size1), 0.9, 1),
                Detection::new(time, BoundingBox::new(1400.0, 400.0, size2, size2), 0.9, 2),
            ]);
        }

        planner.compute_camera_plan(&detections, 0.0, 2.0);

        // Should have at least one switch
        assert!(
            planner.stats().subject_switches >= 1,
            "Should track subject switches"
        );
    }
}
