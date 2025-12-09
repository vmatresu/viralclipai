//! Split mode evaluation logic.
//!
//! Handles the decision-making process for entering split view based on detection frames.
//! Used by `TierAwareSplitProcessor` to decide when to switch from single to split view.

use std::collections::HashMap;
use tracing::info;
use crate::intelligent::models::{BoundingBox, Detection};

/// Evaluator for split view decisions.
pub struct SplitEvaluator;

impl SplitEvaluator {
    /// Evaluate whether we should enter split mode and return per-side boxes.
    ///
    /// # Arguments
    /// * `frames` - Detection history for the segment
    /// * `width` - Frame width
    /// * `height` - Frame height
    /// * `duration` - Segment duration (unused but kept for API compatibility if needed)
    ///
    /// # Returns
    /// `Option<(BoundingBox, BoundingBox)>` - If split is warranted, returns crop boxes
    /// for left (top) and right (bottom) panels.
    pub fn evaluate_speaker_split(
        frames: &[Vec<Detection>],
        width: u32,
        height: u32,
        _duration: f64,
    ) -> Option<(BoundingBox, BoundingBox)> {
        const MARGIN: f64 = 0.25;

        if frames.is_empty() {
            return None;
        }

        // Count frames with "dual activity" (2+ speakers with high confidence)
        // Score > 0.45 serves as a basic confidence threshold for valid face
        let dual_frames = frames
            .iter()
            .filter(|f| f.iter().filter(|d| d.score > 0.45).count() >= 2)
            .count();

        // Heuristic: require at least 3 dual frames OR 50% of frames to have dual speakers
        // This avoids jittery splits on transient false positives
        if dual_frames < 3 || dual_frames * 2 < frames.len() {
            info!(
                dual_frames,
                total_frames = frames.len(),
                "Speaker-aware split: insufficient dual activity, keeping single view"
            );
            return None;
        }

        // Aggregate boxes per track, then deterministically map by center_x:
        // leftmost -> top panel (left in split view source), rightmost -> bottom panel
        let mut track_boxes: HashMap<u32, Vec<BoundingBox>> = HashMap::new();
        for frame in frames {
            for det in frame {
                track_boxes.entry(det.track_id).or_default().push(det.bbox);
            }
        }

        if track_boxes.len() < 2 {
            return None;
        }

        // Compute union box for each track
        let mut tracks: Vec<(u32, BoundingBox)> = track_boxes
            .iter()
            .filter_map(|(id, boxes)| BoundingBox::union(boxes).map(|b| (*id, b)))
            .collect();

        if tracks.len() < 2 {
            return None;
        }

        // Sort by X coordinate (left to right)
        tracks.sort_by(|a, b| {
            a.1
                .cx()
                .partial_cmp(&b.1.cx())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Use the two most prominent (or just two leftmost/rightmost) tracks
        let left_union = tracks[0].1;
        let right_union = tracks[1].1;

        let expand = |b: BoundingBox| {
            let pad = (b.width.max(b.height)) * MARGIN;
            b.pad(pad)
        };
        
        let left_box = expand(left_union).clamp(width, height);
        let right_box = expand(right_union).clamp(width, height);

        Some((left_box, right_box))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_speaker_split_two_speakers_triggers_split() {
        let width = 1920;
        let height = 1080;
        let frames = vec![vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(200.0, 200.0, 200.0, 200.0),
                0.9,
                1,
                Some(0.8),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1400.0, 220.0, 200.0, 200.0),
                0.9,
                2,
                Some(0.8),
            ),
        ]];

        // Pass simple check (1 frame is insufficient by default, but we can verify logic if we mock more frames
        // or just verify the function handles inputs safely. 
        // Logic requires 3 frames min. Let's duplicate.
        let frames = vec![frames[0].clone(), frames[0].clone(), frames[0].clone(), frames[0].clone()];

        let res = SplitEvaluator::evaluate_speaker_split(&frames, width, height, 0.5);
        assert!(res.is_some(), "Should split when both are talking");
        let (left_box, right_box) = res.unwrap();
        assert!(left_box.cx() < right_box.cx());
    }

    #[test]
    fn test_speaker_split_left_top_right_bottom_invariant() {
        let width = 1920;
        let height = 1080;
        let frames = vec![vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(100.0, 200.0, 150.0, 150.0),
                0.9,
                1,
                Some(0.01),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1400.0, 220.0, 150.0, 150.0),
                0.9,
                2,
                Some(0.2),
            ),
        ]];
        
        let frames = vec![frames[0].clone(), frames[0].clone(), frames[0].clone(), frames[0].clone()];

        let res = SplitEvaluator::evaluate_speaker_split(&frames, width, height, 0.5);
        assert!(res.is_some(), "Should enter split mode with two faces");
        let (left_box, right_box) = res.unwrap();
        assert!(left_box.cx() < right_box.cx(), "Left face should map to top panel");
    }
}
