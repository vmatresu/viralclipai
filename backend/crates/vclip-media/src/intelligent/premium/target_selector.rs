//! Camera target selection for the premium intelligent_speaker style.
//!
//! This module implements smart subject selection with:
//! - Primary subject tracking with stability
//! - Vertical bias for eye placement
//! - Multi-speaker hysteresis to prevent ping-ponging
//! - Scene change detection for fast adaptation
//!
//! IMPORTANT: All scoring is PURELY VISUAL - NO audio information is used.

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
    /// Whether this is a scene change frame
    pub is_scene_change: bool,
}

/// Visual activity scores breakdown for debugging.
#[derive(Debug, Clone, Copy, Default)]
pub struct VisualScores {
    pub size_score: f64,
    pub conf_score: f64,
    pub mouth_score: f64,
    pub stability_score: f64,
    pub center_score: f64,
    pub total: f64,
}

/// Subject state for tracking across frames.
#[derive(Debug, Clone)]
struct SubjectState {
    /// Last known bounding box
    bbox: BoundingBox,
    /// Cumulative visual activity score
    activity_score: f64,
    /// Time when this subject became primary
    primary_since: Option<f64>,
    /// First seen time (track age)
    first_seen: f64,
    /// Recent positions for jitter calculation (time, cx, cy)
    position_history: Vec<(f64, f64, f64)>,
    /// Recent mouth activity history (time, score)
    mouth_history: Vec<(f64, f64)>,
    /// Smoothed mouth activity
    smoothed_mouth: f64,
}

impl SubjectState {
    fn new(bbox: BoundingBox, time: f64) -> Self {
        Self {
            bbox,
            activity_score: 0.0,
            primary_since: None,
            first_seen: time,
            position_history: Vec::new(),
            mouth_history: Vec::new(),
            smoothed_mouth: 0.0,
        }
    }

    /// Compute track age in seconds.
    fn age(&self, current_time: f64) -> f64 {
        (current_time - self.first_seen).max(0.0)
    }

    /// Compute positional jitter from recent history.
    fn compute_jitter(&self) -> f64 {
        if self.position_history.len() < 2 {
            return 0.0;
        }

        // Compute variance of positions
        let n = self.position_history.len() as f64;
        let mean_cx: f64 = self
            .position_history
            .iter()
            .map(|(_, cx, _)| cx)
            .sum::<f64>()
            / n;
        let mean_cy: f64 = self
            .position_history
            .iter()
            .map(|(_, _, cy)| cy)
            .sum::<f64>()
            / n;

        let variance: f64 = self
            .position_history
            .iter()
            .map(|(_, cx, cy)| {
                let dx = cx - mean_cx;
                let dy = cy - mean_cy;
                dx * dx + dy * dy
            })
            .sum::<f64>()
            / n;

        variance.sqrt()
    }
}

/// Camera target selector for premium intelligent_speaker style.
///
/// Selects the primary subject to follow and computes focus points
/// with vertical bias and stability constraints.
/// ALL SCORING IS PURELY VISUAL - NO AUDIO.
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
    /// Last scene change time
    last_scene_change_time: Option<f64>,
    /// Last known good focus (for dropout handling)
    last_focus: Option<FocusPoint>,
    /// Time of last valid detection
    last_detection_time: Option<f64>,
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
            last_scene_change_time: None,
            last_focus: None,
            last_detection_time: None,
        }
    }

    /// Select focus point for the current frame.
    ///
    /// Returns a focus point with vertical bias applied, suitable for
    /// camera planning.
    pub fn select_focus(&mut self, detections: &FrameDetections, current_time: f64) -> FocusPoint {
        // Handle empty detections with dropout tolerance
        if detections.is_empty() {
            return self.handle_dropout(current_time);
        }

        // Check for scene change
        let scene_changed = self.detect_scene_change(detections);
        if scene_changed {
            if self.config.enable_debug_logging {
                debug!(
                    "Scene change detected at t={:.2}s, resetting primary subject",
                    current_time
                );
            }
            self.reset_primary_subject();
            self.last_scene_change_time = Some(current_time);
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
        let mut focus = self.compute_focus_point(primary_det, current_time);
        focus.is_scene_change = scene_changed;

        // Update last known good state
        self.last_focus = Some(focus);
        self.last_detection_time = Some(current_time);

        focus
    }

    /// Handle detection dropout - hold last position for a while.
    fn handle_dropout(&mut self, current_time: f64) -> FocusPoint {
        if let (Some(last_focus), Some(last_time)) = (self.last_focus, self.last_detection_time) {
            let dropout_duration = current_time - last_time;

            if dropout_duration <= self.config.max_dropout_hold_sec {
                // Hold last known position
                if self.config.enable_debug_logging {
                    debug!(
                        "Detection dropout at t={:.2}s, holding position ({}s)",
                        current_time, dropout_duration
                    );
                }
                return FocusPoint {
                    cx: last_focus.cx,
                    cy: last_focus.cy,
                    width: last_focus.width,
                    height: last_focus.height,
                    track_id: last_focus.track_id,
                    score: last_focus.score * 0.9, // Slight decay
                    is_scene_change: false,
                };
            }
        }

        // Fallback after extended dropout
        self.fallback_focus(current_time)
    }

    /// Update subject states from current detections.
    fn update_subject_states(&mut self, detections: &FrameDetections, current_time: f64) {
        let stability_window = self.config.stability_window_sec;
        let frame_area = (self.frame_width * self.frame_height) as f64;
        let frame_w = self.frame_width as f64;
        let frame_h = self.frame_height as f64;

        for det in detections {
            let state = self
                .subjects
                .entry(det.track_id)
                .or_insert_with(|| SubjectState::new(det.bbox, current_time));

            // Update bbox
            state.bbox = det.bbox;

            // Update position history for jitter calculation
            state
                .position_history
                .push((current_time, det.bbox.cx(), det.bbox.cy()));
            state
                .position_history
                .retain(|(t, _, _)| current_time - t <= stability_window);

            // Update mouth activity history
            let mouth_val = det.mouth_openness.unwrap_or(0.0);
            state.mouth_history.push((current_time, mouth_val));
            state
                .mouth_history
                .retain(|(t, _)| current_time - t <= stability_window);

            // Compute smoothed mouth activity
            state.smoothed_mouth = Self::smooth_mouth_activity(&state.mouth_history);

            // Compute full visual activity score inline to avoid borrow issues
            let scores = Self::compute_visual_scores_static(
                det,
                state,
                current_time,
                frame_area,
                frame_w,
                frame_h,
                &self.config,
            );
            state.activity_score = scores.total;

            if self.config.enable_debug_logging {
                debug!(
                    "Track {} scores: size={:.2} conf={:.2} mouth={:.2} stab={:.2} center={:.2} -> {:.2}",
                    det.track_id, scores.size_score, scores.conf_score, 
                    scores.mouth_score, scores.stability_score, scores.center_score, scores.total
                );
            }
        }

        // Clean up stale subjects
        let current_ids: Vec<u32> = detections.iter().map(|d| d.track_id).collect();
        self.subjects.retain(|id, _| current_ids.contains(id));

        // Update tracking state
        self.prev_detection_count = detections.len();
        self.prev_track_ids = current_ids;
    }

    /// Compute visual-only activity scores for a detection (static version).
    /// NO AUDIO INFORMATION IS USED.
    fn compute_visual_scores_static(
        det: &Detection,
        state: &SubjectState,
        current_time: f64,
        frame_area: f64,
        frame_w: f64,
        frame_h: f64,
        config: &PremiumSpeakerConfig,
    ) -> VisualScores {
        // 1. Size/prominence score (normalized by frame area)
        let area_ratio = det.bbox.area() / frame_area;
        let size_score = (area_ratio * 10.0).sqrt().min(1.0);

        // 2. Detection confidence score
        let conf_score = det.score.clamp(0.0, 1.0);

        // 3. Mouth/facial activity score (visual only - from face mesh)
        let mouth_score = state.smoothed_mouth.clamp(0.0, 1.0);

        // 4. Track stability score (age + low jitter)
        let age = state.age(current_time);
        let age_factor = (age / config.min_stable_age_sec).min(1.0);

        let jitter = state.compute_jitter();
        let jitter_factor = (1.0 - jitter / config.max_stable_jitter_px)
            .max(0.0)
            .min(1.0);

        let stability_score = age_factor * 0.5 + jitter_factor * 0.5;

        // 5. Geometric centering score
        let cx = det.bbox.cx();
        let cy = det.bbox.cy();

        let h_dist = (cx - frame_w / 2.0).abs() / (frame_w / 2.0);
        let h_center = 1.0 - h_dist;

        let ideal_y = frame_h * 0.35;
        let v_dist = (cy - ideal_y).abs() / frame_h;
        let v_center = (1.0 - v_dist * 1.5).max(0.0);

        let center_score = h_center * 0.6 + v_center * 0.4;

        // Weighted combination (ALL VISUAL)
        let total = config.weight_size * size_score
            + config.weight_confidence * conf_score
            + config.weight_mouth_activity * mouth_score
            + config.weight_track_stability * stability_score
            + config.weight_centering * center_score;

        VisualScores {
            size_score,
            conf_score,
            mouth_score,
            stability_score,
            center_score,
            total,
        }
    }

    /// Compute visual-only activity scores for a detection.
    /// NO AUDIO INFORMATION IS USED.
    fn compute_visual_scores(
        &self,
        det: &Detection,
        state: &SubjectState,
        current_time: f64,
    ) -> VisualScores {
        Self::compute_visual_scores_static(
            det,
            state,
            current_time,
            (self.frame_width * self.frame_height) as f64,
            self.frame_width as f64,
            self.frame_height as f64,
            &self.config,
        )
    }

    /// Smooth mouth activity using EMA over history.
    fn smooth_mouth_activity(history: &[(f64, f64)]) -> f64 {
        if history.is_empty() {
            return 0.0;
        }
        if history.len() == 1 {
            return history[0].1;
        }

        // Simple EMA with alpha=0.3
        let alpha = 0.3;
        let mut smoothed = history[0].1;
        for (_, score) in history.iter().skip(1) {
            smoothed = alpha * score + (1.0 - alpha) * smoothed;
        }
        smoothed
    }

    /// Select the primary subject to follow.
    fn select_primary_subject(&mut self, detections: &FrameDetections, current_time: f64) -> u32 {
        // Check if we're in reacquisition window (after scene change)
        let in_reacquisition = self
            .last_scene_change_time
            .map(|t| self.config.is_in_reacquisition(current_time - t))
            .unwrap_or(false);

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

        // Get effective dwell time (shorter during reacquisition)
        let dwell_time = if in_reacquisition {
            self.config.reacquisition_dwell_time_seconds()
        } else {
            self.config.dwell_time_seconds()
        };

        // Check dwell time
        let time_since_switch = current_time - self.primary_selected_at.unwrap_or(0.0);
        if time_since_switch < dwell_time {
            return current_primary;
        }

        // Check if another subject is significantly more active
        let current_activity = self
            .subjects
            .get(&current_primary)
            .map(|s| s.activity_score)
            .unwrap_or(0.0);

        let best_candidate = self.find_most_active_subject(detections);
        let best_activity = self
            .subjects
            .get(&best_candidate)
            .map(|s| s.activity_score)
            .unwrap_or(0.0);

        let margin = self.config.switch_activity_margin;
        if best_candidate != current_primary && best_activity > current_activity * (1.0 + margin) {
            if self.config.enable_debug_logging {
                debug!(
                    "Switching primary subject: {} -> {} (activity: {:.2} -> {:.2})",
                    current_primary, best_candidate, current_activity, best_activity
                );
            }
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
                let score_a = self
                    .subjects
                    .get(&a.track_id)
                    .map(|s| s.activity_score)
                    .unwrap_or(0.0);
                let score_b = self
                    .subjects
                    .get(&b.track_id)
                    .map(|s| s.activity_score)
                    .unwrap_or(0.0);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
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
    pub fn reset_primary_subject(&mut self) {
        self.primary_subject = None;
        self.primary_selected_at = None;
    }

    /// Detect scene change based on detection layout changes.
    pub fn detect_scene_change(&self, detections: &FrameDetections) -> bool {
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
        let common_count = current_ids
            .iter()
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
        let score = self
            .subjects
            .get(&det.track_id)
            .map(|s| s.activity_score)
            .unwrap_or(det.score);

        FocusPoint {
            cx: base_cx,
            cy: biased_cy,
            width: focus_width,
            height: focus_height,
            track_id: det.track_id,
            score,
            is_scene_change: false,
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
            is_scene_change: false,
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
        self.subjects
            .get(&track_id)
            .map(|s| s.activity_score)
            .unwrap_or(0.0)
    }

    /// Get visual scores breakdown for a track (for debugging).
    pub fn get_visual_scores(&self, det: &Detection, current_time: f64) -> VisualScores {
        if let Some(state) = self.subjects.get(&det.track_id) {
            self.compute_visual_scores(det, state, current_time)
        } else {
            VisualScores::default()
        }
    }

    /// Check if we're in reacquisition window.
    pub fn is_in_reacquisition(&self, current_time: f64) -> bool {
        self.last_scene_change_time
            .map(|t| self.config.is_in_reacquisition(current_time - t))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(time: f64, x: f64, y: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(time, BoundingBox::new(x, y, size, size), 0.9, track_id)
    }

    fn make_detection_with_mouth(
        time: f64,
        x: f64,
        y: f64,
        size: f64,
        track_id: u32,
        mouth: f64,
    ) -> Detection {
        Detection::with_mouth(
            time,
            BoundingBox::new(x, y, size, size),
            0.9,
            track_id,
            Some(mouth),
        )
    }

    #[test]
    fn test_single_subject_selection() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let detections = vec![make_detection(0.0, 800.0, 400.0, 200.0, 1)];

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
            make_detection(0.0, 200.0, 400.0, 150.0, 1),
            make_detection(0.0, 1400.0, 400.0, 250.0, 2),
        ];

        let focus1 = selector.select_focus(&detections, 0.0);
        assert_eq!(focus1.track_id, 2, "Should select larger face initially");

        // Shortly after, face 1 becomes slightly larger - should NOT switch (dwell time)
        let detections2 = vec![
            make_detection(0.5, 200.0, 400.0, 260.0, 1),
            make_detection(0.5, 1400.0, 400.0, 250.0, 2),
        ];

        let focus2 = selector.select_focus(&detections2, 0.5);
        assert_eq!(focus2.track_id, 2, "Should NOT switch before dwell time");
    }

    #[test]
    fn test_vertical_bias() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let detections = vec![make_detection(0.0, 800.0, 400.0, 200.0, 1)];

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

        let focus = selector.select_focus(&detections2, 1.0);
        assert!(focus.track_id == 10 || focus.track_id == 11);
        assert!(focus.is_scene_change);
    }

    #[test]
    fn test_fallback_on_empty() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let focus = selector.select_focus(&vec![], 0.0);

        assert!(focus.cx > 0.0);
        assert!(focus.cy > 0.0);
        assert_eq!(focus.track_id, 0);
    }

    #[test]
    fn test_dropout_handling() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // First frame with detection
        let detections = vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)];
        let focus1 = selector.select_focus(&detections, 0.0);

        // Short dropout - should hold position
        let focus2 = selector.select_focus(&vec![], 0.5);
        assert!(
            (focus2.cx - focus1.cx).abs() < 1.0,
            "Should hold position during short dropout"
        );
        assert_eq!(focus2.track_id, 1);

        // Long dropout - should fallback
        let focus3 = selector.select_focus(&vec![], 2.0);
        assert_eq!(focus3.track_id, 0, "Should fallback after long dropout");
    }

    #[test]
    fn test_visual_scores_no_audio() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Detection with mouth activity (visual signal from face mesh)
        let det = make_detection_with_mouth(0.0, 500.0, 400.0, 200.0, 1, 0.8);
        let detections = vec![det.clone()];

        selector.select_focus(&detections, 0.0);

        let scores = selector.get_visual_scores(&det, 0.0);

        // All scores should be computed from visual signals only
        assert!(scores.size_score >= 0.0 && scores.size_score <= 1.0);
        assert!(scores.conf_score >= 0.0 && scores.conf_score <= 1.0);
        assert!(scores.mouth_score >= 0.0 && scores.mouth_score <= 1.0);
        assert!(scores.stability_score >= 0.0 && scores.stability_score <= 1.0);
        assert!(scores.center_score >= 0.0 && scores.center_score <= 1.0);
    }

    #[test]
    fn test_reacquisition_window() {
        let mut config = PremiumSpeakerConfig::default();
        config.reacquisition_window_sec = 0.5;
        config.reacquisition_dwell_factor = 0.3;

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Trigger scene change
        let det1 = vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)];
        selector.select_focus(&det1, 0.0);

        let det2 = vec![make_detection(0.1, 500.0, 400.0, 200.0, 10)]; // New track
        let focus = selector.select_focus(&det2, 0.1);

        assert!(focus.is_scene_change || selector.is_in_reacquisition(0.1));
    }
}
