//! Core detection pipeline trait and result types.
//!
//! The `DetectionPipeline` trait defines the interface for analyzing video
//! segments and producing per-frame detection results.

use async_trait::async_trait;
use std::path::Path;
use vclip_models::DetectionTier;

use crate::error::MediaResult;
use crate::intelligent::{FrameDetections, SpeakerSegment};

/// Result of analyzing a video segment through a detection pipeline.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Per-frame detection results.
    pub frames: Vec<FrameResult>,

    /// Speaker activity segments (for AudioAware+ tiers).
    pub speaker_segments: Option<Vec<SpeakerSegment>>,

    /// The detection tier that was actually used (may differ from requested
    /// if fallback occurred).
    pub tier_used: DetectionTier,

    /// Video dimensions.
    pub width: u32,
    pub height: u32,

    /// Video frame rate.
    pub fps: f64,

    /// Video duration in seconds.
    pub duration: f64,
}

/// Detection result for a single frame.
#[derive(Debug, Clone)]
pub struct FrameResult {
    /// Timestamp in seconds.
    pub time: f64,

    /// Face detections in this frame.
    pub faces: FrameDetections,

    /// Per-face activity scores (for SpeakerAware tier).
    /// Maps track_id -> activity_score.
    pub activity_scores: Option<Vec<(u32, f64)>>,

    /// Active speaker at this time (for AudioAware+ tiers).
    pub active_speaker: Option<ActiveSpeakerHint>,
}

/// Hint about which speaker is currently active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveSpeakerHint {
    /// Left speaker (in split view layouts).
    Left,
    /// Right speaker (in split view layouts).
    Right,
    /// Both speakers active.
    Both,
    /// No speaker active (silence).
    None,
    /// Single speaker (for single-person videos).
    Single,
}

impl From<crate::intelligent::ActiveSpeaker> for ActiveSpeakerHint {
    fn from(speaker: crate::intelligent::ActiveSpeaker) -> Self {
        match speaker {
            crate::intelligent::ActiveSpeaker::Left => ActiveSpeakerHint::Left,
            crate::intelligent::ActiveSpeaker::Right => ActiveSpeakerHint::Right,
            crate::intelligent::ActiveSpeaker::Both => ActiveSpeakerHint::Both,
            crate::intelligent::ActiveSpeaker::None => ActiveSpeakerHint::None,
        }
    }
}

/// Core trait for detection pipelines.
///
/// Implementations analyze video segments and produce detection results
/// appropriate for their configured detection tier.
#[async_trait]
pub trait DetectionPipeline: Send + Sync {
    /// Analyze a video segment.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file (should be a pre-cut segment)
    /// * `start_time` - Start time in seconds (usually 0.0 for segments)
    /// * `end_time` - End time in seconds (usually segment duration)
    ///
    /// # Returns
    /// Detection results including per-frame data and optional speaker segments.
    async fn analyze(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
    ) -> MediaResult<DetectionResult>;

    /// The detection tier this pipeline implements.
    fn tier(&self) -> DetectionTier;

    /// Human-readable name for logging.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_speaker_hint_conversion() {
        use crate::intelligent::ActiveSpeaker;

        assert_eq!(
            ActiveSpeakerHint::from(ActiveSpeaker::Left),
            ActiveSpeakerHint::Left
        );
        assert_eq!(
            ActiveSpeakerHint::from(ActiveSpeaker::Right),
            ActiveSpeakerHint::Right
        );
        assert_eq!(
            ActiveSpeakerHint::from(ActiveSpeaker::Both),
            ActiveSpeakerHint::Both
        );
        assert_eq!(
            ActiveSpeakerHint::from(ActiveSpeaker::None),
            ActiveSpeakerHint::None
        );
    }
}
