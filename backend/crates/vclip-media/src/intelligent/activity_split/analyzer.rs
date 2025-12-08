//! Activity scoring for Smart Split (Activity).
//!
//! Translates per-frame detections into normalized activity scores that feed the
//! layout planner. The scorer intentionally stays lightweight so it can run on
//! long clips without decoding full frames.

use std::collections::HashMap;

use super::TimelineFrame;
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::models::{BoundingBox, FrameDetections};

/// Builder that converts detections into per-track activity scores.
pub(crate) struct ActivityAnalyzer {
    config: IntelligentCropConfig,
    frame_width: u32,
    frame_height: u32,
    sample_interval: f64,
}

impl ActivityAnalyzer {
    pub fn new(config: IntelligentCropConfig, frame_width: u32, frame_height: u32) -> Self {
        let sample_interval = if config.fps_sample > 0.0 {
            1.0 / config.fps_sample
        } else {
            0.125
        };

        Self {
            config,
            frame_width,
            frame_height,
            sample_interval,
        }
    }

    /// Compute activity timeline from face detections.
    pub fn build_timeline(
        &self,
        detections: &[FrameDetections],
        duration: f64,
    ) -> MediaResult<Vec<TimelineFrame>> {
        if detections.is_empty() {
            return Err(MediaError::detection_failed(
                "Smart Split (Activity) requires face detections across the segment",
            ));
        }

        let mut track_state: HashMap<u32, (BoundingBox, f64)> = HashMap::new();
        let mut frames = Vec::with_capacity(detections.len());
        let mut time = 0.0;

        for frame in detections {
            if time > duration {
                break;
            }

            let mut scores = Vec::new();
            for det in frame {
                let (motion_score, size_score) = if let Some((prev_bbox, prev_area)) =
                    track_state.get(&det.track_id)
                {
                    let motion = self.motion_score(prev_bbox, &det.bbox);
                    let size = self.size_delta_score(*prev_area, det.bbox.area());
                    (motion, size)
                } else {
                    (0.0, 0.0)
                };

                let raw_score = self.combine_scores(motion_score, size_score);
                scores.push((det.track_id, raw_score));
                track_state.insert(det.track_id, (det.bbox, det.bbox.area()));
            }

            // Record frame even if no detections; planner will fail fast later.
            frames.push(TimelineFrame {
                time,
                detections: frame.clone(),
                raw_activity: scores,
            });

            time += self.sample_interval;
        }

        let has_tracks = frames
            .iter()
            .any(|f| !f.raw_activity.is_empty() && !f.detections.is_empty());
        if !has_tracks {
            return Err(MediaError::detection_failed(
                "Smart Split (Activity) could not find any tracked faces to score",
            ));
        }

        Ok(frames)
    }

    fn motion_score(&self, prev: &BoundingBox, current: &BoundingBox) -> f64 {
        let dx = current.cx() - prev.cx();
        let dy = current.cy() - prev.cy();
        let distance = (dx * dx + dy * dy).sqrt();
        let norm = distance / (self.frame_width.max(self.frame_height) as f64);
        (norm * 6.0).clamp(0.0, 1.0)
    }

    fn size_delta_score(&self, prev_area: f64, current_area: f64) -> f64 {
        if prev_area <= 0.0 {
            return 0.0;
        }
        let delta = (current_area - prev_area) / prev_area;
        delta.clamp(0.0, 1.0)
    }

    fn combine_scores(&self, motion: f64, size: f64) -> f64 {
        let weight_motion = self.config.activity_weight_motion.max(0.0);
        let weight_size = self.config.activity_weight_size_change.max(0.0);

        let total = weight_motion + weight_size;
        if total <= 0.0 {
            return 0.0;
        }

        ((motion * weight_motion) + (size * weight_size)) / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_when_no_detections() {
        let analyzer = ActivityAnalyzer::new(IntelligentCropConfig::default(), 1920, 1080);
        let result = analyzer.build_timeline(&[], 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn computes_scores_for_tracks() {
        let analyzer = ActivityAnalyzer::new(IntelligentCropConfig::default(), 1920, 1080);
        let frame = vec![Detection::new(
            0.0,
            BoundingBox::new(100.0, 100.0, 200.0, 200.0),
            0.9,
            1,
        )];
        let timeline = analyzer
            .build_timeline(&[frame], 0.2)
            .expect("timeline should succeed");
        assert_eq!(timeline.len(), 1);
        assert!(!timeline[0].raw_activity.is_empty());
    }
}

