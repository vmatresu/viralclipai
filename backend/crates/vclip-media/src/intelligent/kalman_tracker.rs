//! Kalman Filter-based Face Tracker with Scene-Cut Awareness
//!
//! Implements a multi-object tracker using Kalman filters for smooth
//! face position interpolation between keyframe detections.
//!
//! # Key Features
//! - **Kalman Prediction**: Smooth motion estimation for gap frames
//! - **Scene-Cut Reset**: Hard reset on scene changes to prevent ghost tracks
//! - **Track Management**: Automatic track creation, update, and deletion
//! - **Confidence Decay**: Tracks lose confidence when not updated
//!
//! # State Vector
//! ```text
//! [cx, cy, w, h, vx, vy, vw, vh]
//!  ^center  ^size  ^velocities
//! ```
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::kalman_tracker::KalmanTracker;
//!
//! let mut tracker = KalmanTracker::new();
//!
//! // On keyframe: update with detections
//! let tracks = tracker.update(&detections, timestamp_ms, scene_hash);
//!
//! // On gap frame: predict positions
//! let predictions = tracker.predict(timestamp_ms);
//!
//! // On scene cut: reset all tracks
//! if scene_cut_detected {
//!     tracker.handle_scene_cut(new_scene_hash);
//! }
//! ```

use super::models::BoundingBox;
use tracing::{debug, info, warn};

/// Configuration for Kalman tracker behavior.
#[derive(Debug, Clone)]
pub struct KalmanTrackerConfig {
    /// Maximum age (frames) before track is deleted
    pub max_age: u32,
    /// Minimum hits before track is considered confirmed
    pub min_hits: u32,
    /// IoU threshold for matching detections to tracks
    pub iou_threshold: f64,
    /// Process noise scale for position
    pub process_noise_pos: f64,
    /// Process noise scale for velocity
    pub process_noise_vel: f64,
    /// Measurement noise scale
    pub measurement_noise: f64,
    /// Confidence decay per frame without update
    pub confidence_decay: f64,
}

impl Default for KalmanTrackerConfig {
    fn default() -> Self {
        Self {
            max_age: 30,
            min_hits: 3,
            iou_threshold: 0.3,
            process_noise_pos: 1.0,
            process_noise_vel: 0.1,
            measurement_noise: 1.0,
            confidence_decay: 0.95,
        }
    }
}

/// Individual face track with Kalman filter state.
#[derive(Debug, Clone)]
pub struct FaceTrack {
    /// Unique track identifier
    pub track_id: u32,
    /// Kalman filter state: [cx, cy, w, h, vx, vy, vw, vh]
    state: [f64; 8],
    /// State covariance matrix (diagonal approximation)
    covariance: [f64; 8],
    /// Track age in frames
    pub age: u32,
    /// Number of successful updates
    pub hits: u32,
    /// Frames since last detection update
    pub time_since_update: u32,
    /// Current confidence score
    pub confidence: f64,
    /// Scene hash when track was created
    pub scene_hash: u64,
    /// Whether track is confirmed (has enough hits)
    pub confirmed: bool,
}

impl FaceTrack {
    /// Create a new track from initial detection.
    pub fn new(track_id: u32, bbox: &BoundingBox, confidence: f64, scene_hash: u64) -> Self {
        let cx = bbox.x + bbox.width / 2.0;
        let cy = bbox.y + bbox.height / 2.0;

        Self {
            track_id,
            state: [cx, cy, bbox.width, bbox.height, 0.0, 0.0, 0.0, 0.0],
            covariance: [10.0, 10.0, 10.0, 10.0, 100.0, 100.0, 100.0, 100.0],
            age: 0,
            hits: 1,
            time_since_update: 0,
            confidence,
            scene_hash,
            confirmed: false,
        }
    }

    /// Predict next state using constant velocity model.
    pub fn predict(&mut self, config: &KalmanTrackerConfig) -> BoundingBox {
        // State transition: x' = x + v
        self.state[0] += self.state[4]; // cx += vx
        self.state[1] += self.state[5]; // cy += vy
        self.state[2] += self.state[6]; // w += vw
        self.state[3] += self.state[7]; // h += vh

        // Ensure positive dimensions
        self.state[2] = self.state[2].max(1.0);
        self.state[3] = self.state[3].max(1.0);

        // Update covariance (simplified)
        for i in 0..4 {
            self.covariance[i] += config.process_noise_pos;
        }
        for i in 4..8 {
            self.covariance[i] += config.process_noise_vel;
        }

        self.age += 1;
        self.time_since_update += 1;

        // Decay confidence
        self.confidence *= config.confidence_decay;

        self.get_bbox()
    }

    /// Update state with new detection (Kalman update step).
    pub fn update(&mut self, bbox: &BoundingBox, confidence: f64, config: &KalmanTrackerConfig) {
        let cx = bbox.x + bbox.width / 2.0;
        let cy = bbox.y + bbox.height / 2.0;
        let measurement = [cx, cy, bbox.width, bbox.height];

        // Compute Kalman gain (simplified diagonal)
        let mut kalman_gain = [0.0f64; 4];
        for i in 0..4 {
            let innovation_var = self.covariance[i] + config.measurement_noise;
            kalman_gain[i] = self.covariance[i] / innovation_var;
        }

        // Update state
        for i in 0..4 {
            let innovation = measurement[i] - self.state[i];
            self.state[i] += kalman_gain[i] * innovation;
            // Update velocity estimate
            self.state[i + 4] = kalman_gain[i] * innovation;
        }

        // Update covariance
        for i in 0..4 {
            self.covariance[i] *= 1.0 - kalman_gain[i];
        }

        // Ensure positive dimensions
        self.state[2] = self.state[2].max(1.0);
        self.state[3] = self.state[3].max(1.0);

        self.hits += 1;
        self.time_since_update = 0;
        self.confidence = confidence;

        // Check if track is now confirmed
        if self.hits >= config.min_hits {
            self.confirmed = true;
        }
    }

    /// Get current bounding box from state.
    pub fn get_bbox(&self) -> BoundingBox {
        let cx = self.state[0];
        let cy = self.state[1];
        let w = self.state[2].max(1.0);
        let h = self.state[3].max(1.0);

        BoundingBox::new(cx - w / 2.0, cy - h / 2.0, w, h)
    }

    /// Get current state tuple (bbox, confidence).
    pub fn get_state(&self) -> (BoundingBox, f64) {
        (self.get_bbox(), self.confidence)
    }

    /// Check if track is valid for given scene.
    pub fn is_valid_for_scene(&self, current_scene_hash: u64) -> bool {
        self.scene_hash == current_scene_hash
    }

    /// Check if track should be deleted.
    pub fn should_delete(&self, config: &KalmanTrackerConfig) -> bool {
        self.time_since_update > config.max_age
    }
}

/// Multi-object Kalman tracker with scene-cut awareness.
pub struct KalmanTracker {
    config: KalmanTrackerConfig,
    /// Active tracks
    tracks: Vec<FaceTrack>,
    /// Next track ID to assign
    next_track_id: u32,
    /// Current scene hash
    current_scene_hash: u64,
    /// Total tracks created
    total_tracks_created: u64,
    /// Total scene cuts handled
    scene_cuts_handled: u64,
}

impl KalmanTracker {
    /// Create a new tracker with default configuration.
    pub fn new() -> Self {
        Self::with_config(KalmanTrackerConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: KalmanTrackerConfig) -> Self {
        Self {
            config,
            tracks: Vec::new(),
            next_track_id: 0,
            current_scene_hash: 0,
            total_tracks_created: 0,
            scene_cuts_handled: 0,
        }
    }

    /// Handle scene cut by invalidating all tracks.
    ///
    /// **CRITICAL**: Must be called when scene cut is detected.
    /// Without this, tracks "ghost" across cuts (Person A → Person B).
    pub fn handle_scene_cut(&mut self, new_scene_hash: u64) {
        if self.current_scene_hash != 0 && !self.tracks.is_empty() {
            info!(
                tracks_invalidated = self.tracks.len(),
                old_scene = self.current_scene_hash,
                new_scene = new_scene_hash,
                "Scene cut detected, hard-resetting tracker"
            );
        }

        // HARD RESET: Clear all tracks
        self.tracks.clear();
        self.current_scene_hash = new_scene_hash;
        self.scene_cuts_handled += 1;
    }

    /// Update tracker with new detections (keyframe).
    ///
    /// Matches detections to existing tracks and creates new tracks for unmatched.
    pub fn update(
        &mut self,
        detections: &[(BoundingBox, f64)],
        _timestamp_ms: u64,
        scene_hash: u64,
    ) -> Vec<(u32, BoundingBox, f64)> {
        // Check for scene cut
        if scene_hash != 0 && scene_hash != self.current_scene_hash {
            self.handle_scene_cut(scene_hash);
        }

        // Predict all tracks forward
        for track in &mut self.tracks {
            track.predict(&self.config);
        }

        // Match detections to tracks using IoU
        let (matches, unmatched_dets, unmatched_tracks) = self.match_detections(detections);

        // Update matched tracks
        for (track_idx, det_idx) in matches {
            let (bbox, conf) = &detections[det_idx];
            self.tracks[track_idx].update(bbox, *conf, &self.config);
        }

        // Create new tracks for unmatched detections
        for det_idx in unmatched_dets {
            let (bbox, conf) = &detections[det_idx];
            let track = FaceTrack::new(self.next_track_id, bbox, *conf, scene_hash);
            self.tracks.push(track);
            self.next_track_id += 1;
            self.total_tracks_created += 1;
        }

        // Mark unmatched tracks (they already predicted)
        // They will be deleted when time_since_update exceeds max_age

        // Remove dead tracks
        self.tracks
            .retain(|t| !t.should_delete(&self.config));

        // Return confirmed tracks
        self.get_confirmed_tracks()
    }

    /// Update with scene-cut check convenience method.
    pub fn update_with_scene_check(
        &mut self,
        detections: &[(BoundingBox, f64)],
        timestamp_ms: u64,
        scene_hash: u64,
    ) -> Vec<(u32, BoundingBox, f64)> {
        self.update(detections, timestamp_ms, scene_hash)
    }

    /// Predict track positions for gap frame (no detection).
    pub fn predict(&mut self, _timestamp_ms: u64) -> Vec<(u32, BoundingBox, f64)> {
        for track in &mut self.tracks {
            track.predict(&self.config);
        }

        // Remove dead tracks
        self.tracks
            .retain(|t| !t.should_delete(&self.config));

        self.get_confirmed_tracks()
    }

    /// Get all confirmed track states.
    fn get_confirmed_tracks(&self) -> Vec<(u32, BoundingBox, f64)> {
        self.tracks
            .iter()
            .filter(|t| t.confirmed)
            .map(|t| (t.track_id, t.get_bbox(), t.confidence))
            .collect()
    }

    /// Get number of active tracks.
    pub fn active_count(&self) -> usize {
        self.tracks.len()
    }

    /// Get minimum confidence across active tracks.
    pub fn min_confidence(&self) -> f64 {
        self.tracks
            .iter()
            .filter(|t| t.confirmed)
            .map(|t| t.confidence)
            .fold(1.0, f64::min)
    }

    /// Get track by ID.
    pub fn track_by_id(&self, track_id: u32) -> Option<&FaceTrack> {
        self.tracks.iter().find(|t| t.track_id == track_id)
    }

    /// Iterate over all tracks.
    pub fn tracks(&self) -> impl Iterator<Item = &FaceTrack> {
        self.tracks.iter()
    }

    /// Hard reset - clear all tracks.
    pub fn hard_reset(&mut self) {
        debug!(
            tracks_cleared = self.tracks.len(),
            "Hard reset triggered"
        );
        self.tracks.clear();
        self.current_scene_hash = 0;
    }

    /// Get current scene hash.
    pub fn current_scene_hash(&self) -> u64 {
        self.current_scene_hash
    }

    /// Get statistics.
    pub fn stats(&self) -> TrackerStats {
        TrackerStats {
            active_tracks: self.tracks.len(),
            confirmed_tracks: self.tracks.iter().filter(|t| t.confirmed).count(),
            total_tracks_created: self.total_tracks_created,
            scene_cuts_handled: self.scene_cuts_handled,
        }
    }

    /// Match detections to tracks using IoU.
    fn match_detections(
        &self,
        detections: &[(BoundingBox, f64)],
    ) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
        if self.tracks.is_empty() || detections.is_empty() {
            return (
                Vec::new(),
                (0..detections.len()).collect(),
                (0..self.tracks.len()).collect(),
            );
        }

        // Compute IoU matrix
        let mut iou_matrix = vec![vec![0.0f64; detections.len()]; self.tracks.len()];
        for (i, track) in self.tracks.iter().enumerate() {
            let track_bbox = track.get_bbox();
            for (j, (det_bbox, _)) in detections.iter().enumerate() {
                iou_matrix[i][j] = bbox_iou(&track_bbox, det_bbox);
            }
        }

        // Greedy matching
        let mut matches = Vec::new();
        let mut matched_tracks = vec![false; self.tracks.len()];
        let mut matched_dets = vec![false; detections.len()];

        // Sort by IoU (descending) and match greedily
        let mut candidates: Vec<(usize, usize, f64)> = Vec::new();
        for i in 0..self.tracks.len() {
            for j in 0..detections.len() {
                if iou_matrix[i][j] >= self.config.iou_threshold {
                    candidates.push((i, j, iou_matrix[i][j]));
                }
            }
        }
        candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

        for (track_idx, det_idx, _iou) in candidates {
            if !matched_tracks[track_idx] && !matched_dets[det_idx] {
                matches.push((track_idx, det_idx));
                matched_tracks[track_idx] = true;
                matched_dets[det_idx] = true;
            }
        }

        let unmatched_dets: Vec<usize> = (0..detections.len())
            .filter(|&i| !matched_dets[i])
            .collect();
        let unmatched_tracks: Vec<usize> = (0..self.tracks.len())
            .filter(|&i| !matched_tracks[i])
            .collect();

        (matches, unmatched_dets, unmatched_tracks)
    }
}

impl Default for KalmanTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracker statistics.
#[derive(Debug, Clone, Default)]
pub struct TrackerStats {
    pub active_tracks: usize,
    pub confirmed_tracks: usize,
    pub total_tracks_created: u64,
    pub scene_cuts_handled: u64,
}

/// Compute IoU between two bounding boxes.
fn bbox_iou(a: &BoundingBox, b: &BoundingBox) -> f64 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area_a = a.width * a.height;
    let area_b = b.width * b.height;
    let union = area_a + area_b - intersection;

    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_bbox(x: f64, y: f64, w: f64, h: f64) -> BoundingBox {
        BoundingBox::new(x, y, w, h)
    }

    #[test]
    fn test_new_track() {
        let bbox = create_bbox(100.0, 100.0, 50.0, 60.0);
        let track = FaceTrack::new(0, &bbox, 0.9, 12345);

        assert_eq!(track.track_id, 0);
        assert_eq!(track.hits, 1);
        assert_eq!(track.age, 0);
        assert!((track.confidence - 0.9).abs() < 0.001);
        assert_eq!(track.scene_hash, 12345);
    }

    #[test]
    fn test_track_predict() {
        let bbox = create_bbox(100.0, 100.0, 50.0, 60.0);
        let config = KalmanTrackerConfig::default();
        let mut track = FaceTrack::new(0, &bbox, 0.9, 0);

        // First update to set velocity
        track.update(&create_bbox(110.0, 100.0, 50.0, 60.0), 0.9, &config);

        // Predict should move based on velocity
        let predicted = track.predict(&config);
        assert!(predicted.x > 100.0); // Should have moved right
    }

    #[test]
    fn test_tracker_update() {
        let mut tracker = KalmanTracker::new();
        let detections = vec![
            (create_bbox(100.0, 100.0, 50.0, 60.0), 0.9),
            (create_bbox(300.0, 200.0, 40.0, 50.0), 0.8),
        ];

        let tracks = tracker.update(&detections, 0, 12345);

        // First update - tracks need min_hits to be confirmed
        assert_eq!(tracker.active_count(), 2);
        assert_eq!(tracks.len(), 0); // Not yet confirmed

        // Second update - should confirm tracks
        let tracks = tracker.update(&detections, 33, 12345);
        assert_eq!(tracks.len(), 0); // Need 3 hits

        // Third update
        let tracks = tracker.update(&detections, 66, 12345);
        assert_eq!(tracks.len(), 2); // Now confirmed
    }

    #[test]
    fn test_scene_cut_clears_tracks() {
        let mut tracker = KalmanTracker::new();

        // Create some tracks
        let detections = vec![(create_bbox(100.0, 100.0, 50.0, 60.0), 0.9)];
        tracker.update(&detections, 0, 12345);
        assert_eq!(tracker.active_count(), 1);

        // Scene cut should clear tracks
        tracker.handle_scene_cut(67890);
        assert_eq!(tracker.active_count(), 0);
        assert_eq!(tracker.current_scene_hash(), 67890);
    }

    #[test]
    fn test_scene_cut_via_update() {
        let mut tracker = KalmanTracker::new();

        // Create track in scene 1
        let detections = vec![(create_bbox(100.0, 100.0, 50.0, 60.0), 0.9)];
        tracker.update(&detections, 0, 12345);
        let track_count = tracker.active_count();
        assert_eq!(track_count, 1);

        // Update with different scene should reset
        let new_detections = vec![(create_bbox(200.0, 200.0, 50.0, 60.0), 0.8)];
        tracker.update(&new_detections, 33, 67890);

        // Old track should be gone, new track created
        assert_eq!(tracker.active_count(), 1);
        assert_eq!(tracker.current_scene_hash(), 67890);
    }

    #[test]
    fn test_track_deletion_after_max_age() {
        let config = KalmanTrackerConfig {
            max_age: 3,
            min_hits: 1,
            ..Default::default()
        };
        let mut tracker = KalmanTracker::with_config(config);

        // Create track
        let detections = vec![(create_bbox(100.0, 100.0, 50.0, 60.0), 0.9)];
        tracker.update(&detections, 0, 12345);
        assert_eq!(tracker.active_count(), 1);

        // Predict without update
        tracker.predict(33);
        tracker.predict(66);
        tracker.predict(99);
        tracker.predict(132); // Should delete after max_age

        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_iou_calculation() {
        let a = create_bbox(0.0, 0.0, 100.0, 100.0);
        let b = create_bbox(50.0, 50.0, 100.0, 100.0);

        let iou = bbox_iou(&a, &b);
        // Overlap: 50x50 = 2500
        // Union: 10000 + 10000 - 2500 = 17500
        // IoU: 2500/17500 ≈ 0.143
        assert!((iou - 0.143).abs() < 0.01);
    }

    #[test]
    fn test_min_confidence() {
        let config = KalmanTrackerConfig {
            min_hits: 1,
            ..Default::default()
        };
        let mut tracker = KalmanTracker::with_config(config);

        let detections = vec![
            (create_bbox(100.0, 100.0, 50.0, 60.0), 0.9),
            (create_bbox(300.0, 200.0, 40.0, 50.0), 0.7),
        ];
        tracker.update(&detections, 0, 12345);

        let min_conf = tracker.min_confidence();
        assert!((min_conf - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_stats() {
        let mut tracker = KalmanTracker::new();

        let detections = vec![(create_bbox(100.0, 100.0, 50.0, 60.0), 0.9)];
        tracker.update(&detections, 0, 12345);

        let stats = tracker.stats();
        assert_eq!(stats.active_tracks, 1);
        assert_eq!(stats.total_tracks_created, 1);
        assert_eq!(stats.scene_cuts_handled, 0);
    }
}
