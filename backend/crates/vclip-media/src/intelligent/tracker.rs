//! IoU-based tracker for maintaining face identity across frames.
//!
//! Uses greedy matching by Intersection over Union to track faces
//! between consecutive frames.

use super::models::BoundingBox;
use std::collections::HashMap;

/// Track information.
#[derive(Debug, Clone)]
struct Track {
    /// Last known bounding box
    bbox: BoundingBox,
    /// Frames since last detection
    age: u32,
    /// Whether track is currently active
    active: bool,
}

/// Simple IoU-based tracker for maintaining identity across frames.
///
/// Uses greedy matching by IoU to associate detections between frames.
pub struct IoUTracker {
    /// IoU threshold for matching
    iou_threshold: f64,
    /// Maximum gap frames before track deletion
    max_gap: u32,
    /// Active tracks
    tracks: HashMap<u32, Track>,
    /// Next track ID to assign
    next_track_id: u32,
}

impl IoUTracker {
    /// Create a new tracker.
    pub fn new(iou_threshold: f64, max_gap: u32) -> Self {
        Self {
            iou_threshold,
            max_gap,
            tracks: HashMap::new(),
            next_track_id: 0,
        }
    }

    /// Update tracks with new detections.
    ///
    /// # Arguments
    /// * `detections` - List of (bbox, score) tuples from face detection
    ///
    /// # Returns
    /// List of (track_id, bbox, score) tuples with assigned track IDs
    pub fn update(&mut self, detections: &[(BoundingBox, f64)]) -> Vec<(u32, BoundingBox, f64)> {
        if detections.is_empty() {
            // Age all tracks
            let to_remove: Vec<u32> = self
                .tracks
                .iter_mut()
                .filter_map(|(id, track)| {
                    track.age += 1;
                    track.active = false;
                    if track.age > self.max_gap {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();

            for id in to_remove {
                self.tracks.remove(&id);
            }

            return Vec::new();
        }

        // Match detections to existing tracks using IoU
        let mut matched = Vec::new();
        let mut unmatched_dets: Vec<usize> = (0..detections.len()).collect();
        let mut unmatched_tracks: Vec<u32> = self.tracks.keys().copied().collect();

        // Greedy matching by IoU
        let mut matches: Vec<(usize, u32)> = Vec::new();

        for (det_idx, (bbox, _score)) in detections.iter().enumerate() {
            let mut best_iou = self.iou_threshold;
            let mut best_track: Option<u32> = None;

            for &track_id in &unmatched_tracks {
                if let Some(track) = self.tracks.get(&track_id) {
                    let iou = bbox.iou(&track.bbox);
                    if iou > best_iou {
                        best_iou = iou;
                        best_track = Some(track_id);
                    }
                }
            }

            if let Some(track_id) = best_track {
                matches.push((det_idx, track_id));
                unmatched_dets.retain(|&idx| idx != det_idx);
                unmatched_tracks.retain(|&id| id != track_id);
            }
        }

        // Update matched tracks
        for (det_idx, track_id) in matches {
            let (bbox, score) = detections[det_idx];
            self.tracks.insert(
                track_id,
                Track {
                    bbox,
                    age: 0,
                    active: true,
                },
            );
            matched.push((track_id, bbox, score));
        }

        // Create new tracks for unmatched detections
        for det_idx in unmatched_dets {
            let (bbox, score) = detections[det_idx];
            let track_id = self.next_track_id;
            self.next_track_id += 1;

            self.tracks.insert(
                track_id,
                Track {
                    bbox,
                    age: 0,
                    active: true,
                },
            );
            matched.push((track_id, bbox, score));
        }

        // Age unmatched tracks
        let to_remove: Vec<u32> = unmatched_tracks
            .iter()
            .filter_map(|&track_id| {
                if let Some(track) = self.tracks.get_mut(&track_id) {
                    track.age += 1;
                    track.active = false;
                    if track.age > self.max_gap {
                        return Some(track_id);
                    }
                }
                None
            })
            .collect();

        for id in to_remove {
            self.tracks.remove(&id);
        }

        matched
    }

    /// Reset the tracker state.
    pub fn reset(&mut self) {
        self.tracks.clear();
        self.next_track_id = 0;
    }

    /// Get the number of active tracks.
    pub fn active_track_count(&self) -> usize {
        self.tracks.values().filter(|t| t.active).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_new_detections() {
        let mut tracker = IoUTracker::new(0.3, 10);

        let detections = vec![
            (BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.9),
            (BoundingBox::new(200.0, 200.0, 50.0, 50.0), 0.8),
        ];

        let tracked = tracker.update(&detections);
        assert_eq!(tracked.len(), 2);
        assert_eq!(tracked[0].0, 0); // First track ID
        assert_eq!(tracked[1].0, 1); // Second track ID
    }

    #[test]
    fn test_tracker_matching() {
        let mut tracker = IoUTracker::new(0.3, 10);

        // First frame
        let det1 = vec![(BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.9)];
        let tracked1 = tracker.update(&det1);
        let first_id = tracked1[0].0;

        // Second frame - slightly moved
        let det2 = vec![(BoundingBox::new(105.0, 105.0, 50.0, 50.0), 0.9)];
        let tracked2 = tracker.update(&det2);

        // Should maintain same track ID
        assert_eq!(tracked2[0].0, first_id);
    }

    #[test]
    fn test_tracker_gap_handling() {
        let mut tracker = IoUTracker::new(0.3, 2);

        // First frame
        let det1 = vec![(BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.9)];
        tracker.update(&det1);

        // Empty frames
        tracker.update(&[]);
        tracker.update(&[]);

        // Track should still exist (age = 2, max_gap = 2)
        assert_eq!(tracker.tracks.len(), 1);

        // One more empty frame
        tracker.update(&[]);

        // Track should be deleted (age > max_gap)
        assert_eq!(tracker.tracks.len(), 0);
    }
}
