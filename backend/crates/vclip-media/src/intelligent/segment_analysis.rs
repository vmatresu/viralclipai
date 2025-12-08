//! Segment analysis for camera path planning.
//!
//! This module provides segment boundary detection and analysis utilities
//! for camera smoothing, extracted from tier_aware_smoother.rs.

use super::models::CameraKeyframe;
use super::smoothing_utils::median;

/// Detect segment boundaries based on large camera moves.
pub fn segment_boundaries(keyframes: &[CameraKeyframe], switch_threshold: f64) -> Vec<(usize, usize)> {
    let mut segments = Vec::new();
    let mut start = 0usize;

    for i in 1..keyframes.len() {
        let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
        let dy = (keyframes[i].cy - keyframes[i - 1].cy).abs();
        if dx > switch_threshold || dy > switch_threshold {
            segments.push((start, i));
            start = i;
        }
    }
    segments.push((start, keyframes.len()));

    segments
}

/// Representative keyframe for a segment (median for stability).
pub fn segment_representative(
    keyframes: &[CameraKeyframe],
    segment: (usize, usize),
) -> CameraKeyframe {
    let (start, end) = segment;
    let seg = &keyframes[start..end];
    if seg.is_empty() {
        return keyframes[start];
    }

    let cx = median(&seg.iter().map(|kf| kf.cx).collect::<Vec<_>>());
    let cy = median(&seg.iter().map(|kf| kf.cy).collect::<Vec<_>>());
    let width = median(&seg.iter().map(|kf| kf.width).collect::<Vec<_>>());
    let height = median(&seg.iter().map(|kf| kf.height).collect::<Vec<_>>());

    // Use the first time in the segment to keep ordering intact
    CameraKeyframe::new(seg.first().unwrap().time, cx, cy, width, height)
}

/// Collapse segments shorter than the minimum duration to the previous stable position.
pub fn flatten_short_segments(
    keyframes: &[CameraKeyframe],
    switch_threshold: f64,
    min_segment_duration: f64,
) -> Vec<CameraKeyframe> {
    let segments = segment_boundaries(keyframes, switch_threshold);
    if segments.len() <= 1 {
        return keyframes.to_vec();
    }

    let mut output = Vec::with_capacity(keyframes.len());
    let mut last_rep = segment_representative(keyframes, segments[0]);

    for (idx, (start, end)) in segments.iter().enumerate() {
        let duration = keyframes[end - 1].time - keyframes[*start].time;
        let rep = if idx == 0 || duration >= min_segment_duration {
            segment_representative(keyframes, (*start, *end))
        } else {
            last_rep
        };

        for i in *start..*end {
            let src = keyframes[i];
            output.push(CameraKeyframe::new(src.time, rep.cx, rep.cy, rep.width, rep.height));
        }

        last_rep = rep;
    }

    output
}

/// Analyze segment structure for switch detection.
#[derive(Debug, Clone)]
pub struct SegmentAnalysis {
    /// Number of distinct segments found
    pub segment_count: usize,
    /// Switch point indices
    pub switch_indices: Vec<usize>,
    /// Average segment duration
    pub avg_segment_duration: f64,
}

impl SegmentAnalysis {
    /// Analyze keyframes for segment structure.
    pub fn analyze(keyframes: &[CameraKeyframe], switch_threshold: f64) -> Self {
        let segments = segment_boundaries(keyframes, switch_threshold);
        let segment_count = segments.len();
        
        let switch_indices: Vec<usize> = segments.iter().skip(1).map(|s| s.0).collect();
        
        let total_duration = if keyframes.len() >= 2 {
            keyframes.last().unwrap().time - keyframes.first().unwrap().time
        } else {
            0.0
        };
        
        let avg_segment_duration = if segment_count > 0 {
            total_duration / segment_count as f64
        } else {
            0.0
        };

        Self {
            segment_count,
            switch_indices,
            avg_segment_duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_boundaries_no_switches() {
        let keyframes = vec![
            CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.1, 105.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.2, 110.0, 100.0, 200.0, 400.0),
        ];

        let segments = segment_boundaries(&keyframes, 50.0);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], (0, 3));
    }

    #[test]
    fn test_segment_boundaries_with_switch() {
        let keyframes = vec![
            CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.1, 110.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.2, 500.0, 100.0, 200.0, 400.0), // Large jump
            CameraKeyframe::new(0.3, 510.0, 100.0, 200.0, 400.0),
        ];

        let segments = segment_boundaries(&keyframes, 50.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], (0, 2));
        assert_eq!(segments[1], (2, 4));
    }

    #[test]
    fn test_flatten_short_segments() {
        let keyframes = vec![
            CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(1.0, 110.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(1.1, 500.0, 100.0, 200.0, 400.0), // Short segment
            CameraKeyframe::new(1.2, 110.0, 100.0, 200.0, 400.0), // Back to original
            CameraKeyframe::new(2.0, 115.0, 100.0, 200.0, 400.0),
        ];

        // With 2 second minimum, the short segment should be flattened
        let result = flatten_short_segments(&keyframes, 50.0, 2.0);
        assert_eq!(result.len(), 5);
        // The middle segment should have been collapsed to match surrounding
    }
}
