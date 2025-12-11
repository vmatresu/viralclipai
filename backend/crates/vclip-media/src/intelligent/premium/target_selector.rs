//! Camera target selection for the premium intelligent_speaker style.
//!
//! This module implements smart subject selection with:
//! - Primary subject tracking with stability
//! - Vertical bias for eye placement
//! - Multi-speaker hysteresis to prevent ping-ponging
//! - Scene change detection for fast adaptation

use std::collections::HashMap;
use tracing::debug;

use super::config::PremiumSpeakerConfig;
use crate::intelligent::models::{BoundingBox, CameraKeyframe, Detection, FrameDetections};

/// Focus point computed from subject detection.
#[derive(Debug, Clone, Copy)]
pub struct FocusPoint {
    /// Center X coordinate
    pub cx: f64,
    /// Center Y coordinate (with vertical bias applied)
    pub cy: f64,
    /// Suggested focus region width
    pub width: f64,
    /// Suggested focus region height
    pub height: f64,
    /// Track ID of the primary subject
    pub track_id: u32,
    /// Confidence/activity score of the selection
    pub score: f64,
}

/// Subject state for tracking across frames.
#[derive(Debug, Clone)]
struct SubjectState {
    /// Last known bounding box
    bbox: BoundingBox,
    /// Cumulative activity score
    activity_score: f64,
    /// Time when this subject became primary
    primary_since: Option<f64>,
    /// Recent activity history (time, score)
    activity_history: Vec<(f64, f64)>,
}

/// Camera target selector for premium intelligent_speaker style.
///
/// Selects the primary subject to follow and computes focus points
/// with vertical bias and stability constraints.
pub struct CameraTargetSelector {
    config: PremiumSpeakerConfig,
    /// Current primary subject track ID
    primary_subject: Option<u32>,
    /// Time when primary subject was selected
    primary_selected_at: Option<f64>,
    /// Per-subject state
    subjects: HashMap<u32, SubjectState>,
    /// Previous frame's detection count (for scene change detection)
    prev_detection_count: usize,
    /// Previous frame's track IDs
    prev_track_ids: Vec<u32>,
    /// Frame dimensions
    frame_width: u32,
    frame_height: u32,
}

impl CameraTargetSelector {
    /// Create a new camera target selector.
    pub fn new(config: PremiumSpeakerConfig, frame_width: u32, frame_height: u32) -> Self {
        Self {
            config,
            primary_subject: None,
            primary_selected_at: None,
            subjects: HashMap::new(),
            prev_detection_count: 0,
            prev_track_ids: Vec::new(),
            frame_width,
            frame_height,
        }
    }

    /// Select focus point for the current frame.
    ///
    /// Returns a focus point with vertical bias applied, suitable for
    /// camera planning.
    pub fn select_focus(
        &mut self,
        detections: &FrameDetections,
        current_time: f64,
    ) -> FocusPoint {
        // Handle empty detections
        if detections.is_empty() {
            return self.fallback_focus(current_time);
        }

        // Check for scene change
        let scene_changed = self.detect_scene_change(detections);
        if scene_changed {
            debug!("Scene change detected at t={:.2}s, resetting primary subject", current_time);
            self.reset_primary_subject();
        }

        // Update subject states
        self.update_subject_states(detections, current_time);

        // Select primary subject
        let primary_id = self.select_primary_subject(detections, current_time);

        // Find the detection for primary subject
        let primary_det = detections
            .iter()
            .find(|d| d.track_id == primary_id)
            .unwrap_or(&detections[0]);

        // Compute focus point with vertical bias
        self.compute_focus_point(primary_det, current_time)
    }

    /// Update subject states from current detections.
    fn update_subject_states(&mut self, detections: &FrameDetections, current_time: f64) {
        let activity_window = self.config.smoothing_time_window_ms as f64 / 1000.0;
        let ema_alpha = self.config.ema_alpha;

        for det in detections {
            // Compute activity score for this frame first (before borrowing subjects)
            let frame_activity = self.compute_subject_activity(det);

            let state = self.subjects.entry(det.track_id).or_insert_with(|| SubjectState {
                bbox: det.bbox,
                activity_score: 0.0,
                primary_since: None,
                activity_history: Vec::new(),
            });

            // Update bbox
            state.bbox = det.bbox;

            // Add to history
            state.activity_history.push((current_time, frame_activity));

            // Prune old history
            state.activity_history.retain(|(t, _)| current_time - t <= activity_window);

            // Compute smoothed activity score inline
            state.activity_score = Self::smooth_activity_inline(&state.activity_history, ema_alpha);
        }

        // Clean up stale subjects
        let current_ids: Vec<u32> = detections.iter().map(|d| d.track_id).collect();
        self.subjects.retain(|id, _| current_ids.contains(id));

        // Update tracking state
        self.prev_detection_count = detections.len();
        self.prev_track_ids = current_ids;
    }

    /// Compute activity score for a single detection.
    fn compute_subject_activity(&self, det: &Detection) -> f64 {
        let frame_area = (self.frame_width * self.frame_height) as f64;

        // Size component (normalized by frame area)
        let size_score = (det.bbox.area() / frame_area).sqrt().min(1.0);

        // Confidence component
        let conf_score = det.score;

        // Mouth activity component (if available)
        let mouth_score = det.mouth_openness.unwrap_or(0.0).min(1.0);

        // Weighted combination
        self.config.weight_face_size * size_score
            + self.config.weight_confidence * conf_score
            + self.config.weight_mouth_activity * mouth_score
    }

    /// Smooth activity scores using EMA (static version for borrow checker).
    fn smooth_activity_inline(history: &[(f64, f64)], alpha: f64) -> f64 {
        if history.is_empty() {
            return 0.0;
        }
        if history.len() == 1 {
            return history[0].1;
        }

        let mut smoothed = history[0].1;
        for (_, score) in history.iter().skip(1) {
            smoothed = alpha * score + (1.0 - alpha) * smoothed;
        }
        smoothed
    }

    /// Smooth activity scores using EMA.
    #[allow(dead_code)]
    fn smooth_activity(&self, history: &[(f64, f64)]) -> f64 {
        Self::smooth_activity_inline(history, self.config.ema_alpha)
    }

    /// Select the primary subject to follow.
    fn select_primary_subject(&mut self, detections: &FrameDetections, current_time: f64) -> u32 {
        // If no current primary, select the most active
        if self.primary_subject.is_none() {
            let best = self.find_most_active_subject(detections);
            self.set_primary_subject(best, current_time);
            return best;
        }

        let current_primary = self.primary_subject.unwrap();

        // Check if current primary still exists
        let primary_exists = detections.iter().any(|d| d.track_id == current_primary);
        if !primary_exists {
            let best = self.find_most_active_subject(detections);
            self.set_primary_subject(best, current_time);
            return best;
        }

        // Check dwell time
        let dwell_time = self.config.dwell_time_seconds();
        let time_since_switch = current_time - self.primary_selected_at.unwrap_or(0.0);
        if time_since_switch < dwell_time {
            return current_primary;
        }

        // Check if another subject is significantly more active
        let current_activity = self.subjects.get(&current_primary)
            .map(|s| s.activity_score)
            .unwrap_or(0.0);

        let best_candidate = self.find_most_active_subject(detections);
        let best_activity = self.subjects.get(&best_candidate)
            .map(|s| s.activity_score)
            .unwrap_or(0.0);

        let margin = self.config.switch_activity_margin;
        if best_candidate != current_primary && best_activity > current_activity * (1.0 + margin) {
            debug!(
                "Switching primary subject: {} -> {} (activity: {:.2} -> {:.2})",
                current_primary, best_candidate, current_activity, best_activity
            );
            self.set_primary_subject(best_candidate, current_time);
            return best_candidate;
        }

        current_primary
    }

    /// Find the most active subject from detections.
    fn find_most_active_subject(&self, detections: &FrameDetections) -> u32 {
        detections
            .iter()
            .max_by(|a, b| {
                let score_a = self.subjects.get(&a.track_id)
                    .map(|s| s.activity_score)
                    .unwrap_or(0.0);
                let score_b = self.subjects.get(&b.track_id)
                    .map(|s| s.activity_score)
                    .unwrap_or(0.0);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.track_id)
            .unwrap_or(0)
    }

    /// Set the primary subject.
    fn set_primary_subject(&mut self, track_id: u32, time: f64) {
        self.primary_subject = Some(track_id);
        self.primary_selected_at = Some(time);

        if let Some(state) = self.subjects.get_mut(&track_id) {
            state.primary_since = Some(time);
        }
    }

    /// Reset primary subject selection (e.g., on scene change).
    fn reset_primary_subject(&mut self) {
        self.primary_subject = None;
        self.primary_selected_at = None;
    }

    /// Detect scene change based on detection layout changes.
    fn detect_scene_change(&self, detections: &FrameDetections) -> bool {
        if !self.config.enable_scene_detection {
            return false;
        }

        // Check for significant change in number of detections
        let count_diff = (detections.len() as i32 - self.prev_detection_count as i32).abs();
        if count_diff >= 2 {
            return true;
        }

        // Check for significant change in track IDs
        let current_ids: Vec<u32> = detections.iter().map(|d| d.track_id).collect();
        let common_count = current_ids.iter()
            .filter(|id| self.prev_track_ids.contains(id))
            .count();

        let total = self.prev_track_ids.len().max(current_ids.len());
        if total > 0 {
            let overlap_ratio = common_count as f64 / total as f64;
            if overlap_ratio < (1.0 - self.config.scene_change_threshold) {
                return true;
            }
        }

        false
    }

    /// Compute focus point with vertical bias.
    fn compute_focus_point(&self, det: &Detection, _time: f64) -> FocusPoint {
        let bbox = &det.bbox;

        // Base focus on face center
        let base_cx = bbox.cx();
        let base_cy = bbox.cy();

        // Apply vertical bias to place eyes in upper third
        // Shift the focus point DOWN so the crop window moves UP relative to the face
        let vertical_shift = bbox.height * self.config.vertical_bias_fraction;
        let biased_cy = base_cy + vertical_shift;

        // Compute focus region size with padding
        let padding = bbox.width * self.config.min_horizontal_padding;
        let focus_width = bbox.width + 2.0 * padding;
        let focus_height = bbox.height * (1.0 + self.config.headroom_ratio);

        // Get activity score
        let score = self.subjects.get(&det.track_id)
            .map(|s| s.activity_score)
            .unwrap_or(det.score);

        FocusPoint {
            cx: base_cx,
            cy: biased_cy,
            width: focus_width,
            height: focus_height,
            track_id: det.track_id,
            score,
        }
    }

    /// Generate fallback focus when no detections.
    fn fallback_focus(&self, _time: f64) -> FocusPoint {
        let w = self.frame_width as f64;
        let h = self.frame_height as f64;

        // Upper-center fallback (TikTok style)
        FocusPoint {
            cx: w / 2.0,
            cy: h * 0.4, // Upper portion
            width: w * 0.6,
            height: h * 0.5,
            track_id: 0,
            score: 0.0,
        }
    }

    /// Convert focus point to camera keyframe.
    pub fn focus_to_keyframe(&self, focus: &FocusPoint, time: f64) -> CameraKeyframe {
        CameraKeyframe::new(time, focus.cx, focus.cy, focus.width, focus.height)
    }

    /// Get the current primary subject ID.
    pub fn current_primary(&self) -> Option<u32> {
        self.primary_subject
    }

    /// Get activity score for a track.
    pub fn get_activity(&self, track_id: u32) -> f64 {
        self.subjects.get(&track_id)
            .map(|s| s.activity_score)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(time: f64, x: f64, y: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(
            time,
            BoundingBox::new(x, y, size, size),
            0.9,
            track_id,
        )
    }

    #[test]
    fn test_single_subject_selection() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let detections = vec![
            make_detection(0.0, 800.0, 400.0, 200.0, 1),
        ];

        let focus = selector.select_focus(&detections, 0.0);
        assert_eq!(focus.track_id, 1);
        assert!(focus.cx > 0.0);
    }

    #[test]
    fn test_multi_subject_stability() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Initial selection - larger face should be primary
        let detections = vec![
            make_detection(0.0, 200.0, 400.0, 150.0, 1), // Smaller
            make_detection(0.0, 1400.0, 400.0, 250.0, 2), // Larger
        ];

        let focus1 = selector.select_focus(&detections, 0.0);
        assert_eq!(focus1.track_id, 2, "Should select larger face initially");

        // Shortly after, face 1 becomes slightly larger - should NOT switch (dwell time)
        let detections2 = vec![
            make_detection(0.5, 200.0, 400.0, 260.0, 1), // Now larger
            make_detection(0.5, 1400.0, 400.0, 250.0, 2),
        ];

        let focus2 = selector.select_focus(&detections2, 0.5);
        assert_eq!(focus2.track_id, 2, "Should NOT switch before dwell time");
    }

    #[test]
    fn test_vertical_bias() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let detections = vec![
            make_detection(0.0, 800.0, 400.0, 200.0, 1),
        ];

        let focus = selector.select_focus(&detections, 0.0);

        // Focus cy should be shifted down from face center
        let face_cy = 400.0 + 100.0; // y + height/2
        assert!(focus.cy > face_cy, "Vertical bias should shift focus down");
    }

    #[test]
    fn test_scene_change_detection() {
        let mut config = PremiumSpeakerConfig::default();
        config.enable_scene_detection = true;

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // First frame
        let detections1 = vec![
            make_detection(0.0, 200.0, 400.0, 200.0, 1),
            make_detection(0.0, 1400.0, 400.0, 200.0, 2),
        ];
        selector.select_focus(&detections1, 0.0);

        // Scene change - completely different faces
        let detections2 = vec![
            make_detection(1.0, 500.0, 300.0, 180.0, 10),
            make_detection(1.0, 1200.0, 300.0, 180.0, 11),
        ];

        // This should trigger scene change and reset primary
        let focus = selector.select_focus(&detections2, 1.0);
        assert!(focus.track_id == 10 || focus.track_id == 11);
    }

    #[test]
    fn test_fallback_on_empty() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let focus = selector.select_focus(&[], 0.0);

        // Should return centered fallback
        assert!(focus.cx > 0.0);
        assert!(focus.cy > 0.0);
        assert_eq!(focus.track_id, 0);
    }
}
