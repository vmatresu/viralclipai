//! Temporal activity tracking and active-face selection.
//!
//! This module aggregates per-face activity scores over time windows,
//! fuses visual activity with audio speech detection, and implements
//! hysteresis-based switching logic to prevent jittery face changes.
//!
//! Mirrors the Python `TemporalActivityTracker` class.

use std::collections::HashMap;
use tracing::debug;

use super::face_activity::FaceActivityConfig;

/// Temporal activity tracker with audio fusion and switching logic.
pub struct TemporalActivityTracker {
    /// Activity history per track ID: (time, visual_score, audio_score)
    activity_history: HashMap<u32, Vec<(f64, f64, f64)>>,

    /// Currently selected active face
    current_face: Option<u32>,

    /// Time when current face became active
    current_face_start_time: Option<f64>,

    /// Configuration
    config: FaceActivityConfig,
}

impl TemporalActivityTracker {
    /// Create a new temporal activity tracker.
    pub fn new(config: FaceActivityConfig) -> Self {
        Self {
            activity_history: HashMap::new(),
            current_face: None,
            current_face_start_time: None,
            config,
        }
    }

    /// Update activity score for a track.
    ///
    /// # Arguments
    /// * `track_id` - Face track ID
    /// * `visual_score` - Visual activity score (0.0-1.0)
    /// * `audio_score` - Speech activity score (0.0-1.0), 0.0 if unknown
    /// * `time` - Current timestamp in seconds
    pub fn update_activity(
        &mut self,
        track_id: u32,
        visual_score: f64,
        audio_score: f64,
        time: f64,
    ) {
        let history = self.activity_history.entry(track_id).or_default();
        history.push((time, visual_score, audio_score));

        // Keep only recent history within activity window
        let window_start = time - self.config.activity_window;
        history.retain(|(t, _, _)| *t >= window_start);
    }

    /// Get average visual activity score for a track over the activity window.
    pub fn get_average_visual_activity(&self, track_id: u32, current_time: f64) -> f64 {
        match self.activity_history.get(&track_id) {
            Some(history) => {
                let window_start = current_time - self.config.activity_window;
                let recent: Vec<f64> = history
                    .iter()
                    .filter(|(t, _, _)| *t >= window_start)
                    .map(|(_, v, _)| *v)
                    .collect();

                if recent.is_empty() {
                    return 0.0;
                }

                self.smooth_scores(&recent)
            }
            None => 0.0,
        }
    }

    /// Get average audio activity score for a track.
    pub fn get_average_audio_activity(&self, track_id: u32, current_time: f64) -> f64 {
        match self.activity_history.get(&track_id) {
            Some(history) => {
                let window_start = current_time - self.config.activity_window;
                let recent: Vec<f64> = history
                    .iter()
                    .filter(|(t, _, _)| *t >= window_start)
                    .map(|(_, _, a)| *a)
                    .collect();

                if recent.is_empty() {
                    return 0.0;
                }

                self.smooth_scores(&recent)
            }
            None => 0.0,
        }
    }

    /// Apply exponential moving average smoothing to scores.
    fn smooth_scores(&self, scores: &[f64]) -> f64 {
        if scores.is_empty() {
            return 0.0;
        }

        if scores.len() == 1 {
            return scores[0];
        }

        // EMA smoothing
        let alpha = self.config.smoothing_alpha;
        let mut smoothed = scores[0];
        for &score in &scores[1..] {
            smoothed = alpha * score + (1.0 - alpha) * smoothed;
        }

        smoothed
    }

    /// Compute final fused score combining visual and audio activity.
    ///
    /// Formula: `visual * (0.5 + 0.5 * audio)`
    /// - Visual-only activity is down-weighted but not ignored
    /// - Speech boosts the active speaker
    pub fn compute_final_score(&self, visual_score: f64, audio_score: f64) -> f64 {
        visual_score * (0.5 + 0.5 * audio_score)
    }

    /// Get final fused activity score for a track.
    pub fn get_final_activity(&self, track_id: u32, current_time: f64) -> f64 {
        let visual = self.get_average_visual_activity(track_id, current_time);
        let audio = self.get_average_audio_activity(track_id, current_time);
        self.compute_final_score(visual, audio)
    }

    /// Select the most active face, respecting minimum switch duration.
    ///
    /// # Arguments
    /// * `available_tracks` - List of track IDs currently detected
    /// * `current_time` - Current timestamp in seconds
    ///
    /// # Returns
    /// Selected track ID or None if no tracks available
    pub fn select_active_face(
        &mut self,
        available_tracks: &[u32],
        current_time: f64,
    ) -> Option<u32> {
        if available_tracks.is_empty() {
            self.current_face = None;
            self.current_face_start_time = None;
            return None;
        }

        // Compute activity for each track
        let track_activities: Vec<(u32, f64)> = available_tracks
            .iter()
            .map(|&id| (id, self.get_final_activity(id, current_time)))
            .collect();

        // Find most active track
        let (best_track, best_activity) = track_activities
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .copied()
            .unwrap_or((available_tracks[0], 0.0));

        // No current face - select best
        if self.current_face.is_none() {
            debug!(
                "Initial active face selection: track {} (activity: {:.2})",
                best_track, best_activity
            );
            self.current_face = Some(best_track);
            self.current_face_start_time = Some(current_time);
            return Some(best_track);
        }

        let current_id = self.current_face.unwrap();

        // Check if current face still exists
        if !available_tracks.contains(&current_id) {
            debug!(
                "Current face {} lost, switching to {} (activity: {:.2})",
                current_id, best_track, best_activity
            );
            self.current_face = Some(best_track);
            self.current_face_start_time = Some(current_time);
            return Some(best_track);
        }

        // Check minimum switch duration
        if let Some(start_time) = self.current_face_start_time {
            let time_since_switch = current_time - start_time;
            if time_since_switch < self.config.min_switch_duration {
                // Too soon to switch, keep current
                return Some(current_id);
            }
        }

        // Check if best track is significantly better
        let current_activity = track_activities
            .iter()
            .find(|(id, _)| *id == current_id)
            .map(|(_, a)| *a)
            .unwrap_or(0.0);

        let activity_difference = best_activity - current_activity;

        // Switch if significantly better (margin threshold)
        if best_track != current_id && activity_difference > self.config.switch_margin {
            debug!(
                "Switching active face: {} -> {} (improvement: {:.2})",
                current_id, best_track, activity_difference
            );
            self.current_face = Some(best_track);
            self.current_face_start_time = Some(current_time);
            return Some(best_track);
        }

        // Keep current face
        Some(current_id)
    }

    /// Get the currently active face without updating state.
    pub fn current_active_face(&self) -> Option<u32> {
        self.current_face
    }

    /// Clean up resources for a track.
    pub fn cleanup_track(&mut self, track_id: u32) {
        self.activity_history.remove(&track_id);
        if self.current_face == Some(track_id) {
            self.current_face = None;
            self.current_face_start_time = None;
        }
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.activity_history.clear();
        self.current_face = None;
        self.current_face_start_time = None;
    }

    /// Get all tracked face IDs.
    pub fn tracked_faces(&self) -> Vec<u32> {
        self.activity_history.keys().copied().collect()
    }
}

impl Default for TemporalActivityTracker {
    fn default() -> Self {
        Self::new(FaceActivityConfig::default())
    }
}

/// Activity score for a face at a point in time.
#[derive(Debug, Clone, Copy)]
pub struct ActivityScore {
    /// Face track ID
    pub track_id: u32,

    /// Timestamp in seconds
    pub time: f64,

    /// Mouth activity component (0.0-1.0)
    pub mouth_activity: f64,

    /// Motion activity component (0.0-1.0)
    pub motion_activity: f64,

    /// Size change activity component (0.0-1.0)
    pub size_activity: f64,

    /// Combined visual score (0.0-1.0)
    pub visual_combined: f64,

    /// Final fused score with audio (0.0-1.0)
    pub final_score: f64,
}

impl ActivityScore {
    /// Create a new activity score.
    pub fn new(
        track_id: u32,
        time: f64,
        mouth: f64,
        motion: f64,
        size: f64,
        config: &FaceActivityConfig,
    ) -> Self {
        // Weighted combination
        let total_weight = config.weight_mouth + config.weight_motion + config.weight_size;
        let visual_combined = if total_weight > 0.0 {
            (mouth * config.weight_mouth + motion * config.weight_motion + size * config.weight_size)
                / total_weight
        } else {
            0.0
        };

        Self {
            track_id,
            time,
            mouth_activity: mouth,
            motion_activity: motion,
            size_activity: size,
            visual_combined,
            final_score: visual_combined, // Audio fusion applied separately
        }
    }

    /// Apply audio fusion to compute final score.
    pub fn with_audio(mut self, audio_score: f64) -> Self {
        self.final_score = self.visual_combined * (0.5 + 0.5 * audio_score);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> FaceActivityConfig {
        FaceActivityConfig {
            activity_window: 0.5,
            min_switch_duration: 1.0,
            switch_margin: 0.2,
            ..Default::default()
        }
    }

    #[test]
    fn test_select_active_face_initial() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Add some activity
        tracker.update_activity(1, 0.8, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);

        let selected = tracker.select_active_face(&[1, 2], 0.0);
        assert_eq!(selected, Some(1), "Should select most active face");
    }

    #[test]
    fn test_min_switch_duration_enforced() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Initial selection
        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);
        tracker.select_active_face(&[1, 2], 0.0);

        // Face 2 becomes much more active, but too soon to switch
        tracker.update_activity(2, 0.9, 0.0, 0.5);
        let selected = tracker.select_active_face(&[1, 2], 0.5);
        assert_eq!(selected, Some(1), "Should not switch before min_switch_duration");

        // After min_switch_duration, should switch
        tracker.update_activity(2, 0.9, 0.0, 1.5);
        let selected = tracker.select_active_face(&[1, 2], 1.5);
        assert_eq!(selected, Some(2), "Should switch after min_switch_duration");
    }

    #[test]
    fn test_margin_required_for_switch() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Initial selection
        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);
        tracker.select_active_face(&[1, 2], 0.0);

        // Face 2 slightly better, but not enough margin
        tracker.update_activity(1, 0.5, 0.0, 1.5);
        tracker.update_activity(2, 0.6, 0.0, 1.5); // Only 0.1 improvement, margin is 0.2
        let selected = tracker.select_active_face(&[1, 2], 1.5);
        assert_eq!(selected, Some(1), "Should not switch without sufficient margin");

        // Face 2 significantly better
        tracker.update_activity(2, 0.9, 0.0, 1.6);
        let selected = tracker.select_active_face(&[1, 2], 1.6);
        assert_eq!(selected, Some(2), "Should switch with sufficient margin");
    }

    #[test]
    fn test_audio_fusion() {
        let tracker = TemporalActivityTracker::new(test_config());

        // Visual only
        let score_no_audio = tracker.compute_final_score(0.8, 0.0);
        assert!((score_no_audio - 0.4).abs() < 0.01, "No audio: 0.8 * 0.5 = 0.4");

        // Visual + audio
        let score_with_audio = tracker.compute_final_score(0.8, 1.0);
        assert!((score_with_audio - 0.8).abs() < 0.01, "With audio: 0.8 * 1.0 = 0.8");
    }

    #[test]
    fn test_cleanup_track() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.select_active_face(&[1], 0.0);

        assert_eq!(tracker.current_face, Some(1));
        assert!(tracker.activity_history.contains_key(&1));

        tracker.cleanup_track(1);

        assert_eq!(tracker.current_face, None);
        assert!(!tracker.activity_history.contains_key(&1));
    }

    #[test]
    fn test_activity_score_creation() {
        let config = test_config();
        let score = ActivityScore::new(1, 0.5, 0.8, 0.4, 0.2, &config);

        assert_eq!(score.track_id, 1);
        assert_eq!(score.time, 0.5);
        assert_eq!(score.mouth_activity, 0.8);
        assert!(score.visual_combined > 0.0);

        let with_audio = score.with_audio(0.8);
        assert!(with_audio.final_score > score.final_score);
    }
}
