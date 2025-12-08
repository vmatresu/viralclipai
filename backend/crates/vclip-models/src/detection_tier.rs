//! Detection tier definitions for intelligent video processing.
//!
//! This module defines the detection tiers that control which detection
//! providers are used during video processing:
//!
//! - `None`: Heuristic positioning only (fastest)
//! - `Basic`: YuNet face detection
//! - `AudioAware`: YuNet + stereo audio activity detection (requires stereo audio)
//! - `SpeakerAware`: YuNet + stereo audio + face activity analysis (requires stereo audio)
//! - `MotionAware`: YuNet + visual motion detection (works with any audio)
//! - `ActivityAware`: YuNet + full visual activity analysis (works with any audio)

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Detection tier for intelligent video processing.
///
/// Controls which detection providers are used during processing.
/// Higher tiers provide better quality but require more processing time.
///
/// Audio-based tiers (AudioAware, SpeakerAware) require stereo audio with speaker panning.
/// Motion-based tiers (MotionAware, ActivityAware) work with any audio format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectionTier {
    /// No AI detection - uses heuristic positioning only.
    /// Fastest processing, deterministic results.
    #[default]
    None,

    /// YuNet face detection only.
    /// Good balance of speed and quality.
    Basic,

    /// YuNet face detection + stereo audio activity detection.
    /// Uses audio channel separation to identify active speakers.
    /// Requires stereo audio with speaker panning (left/right).
    AudioAware,

    /// Full detection stack: YuNet + stereo audio + face activity.
    /// Best quality for stereo audio sources.
    /// Uses mouth movement and motion analysis for speaker tracking.
    SpeakerAware,

    /// YuNet face detection + visual motion detection.
    /// Uses frame differencing to detect active faces.
    /// Works with any audio format (mono/stereo).
    MotionAware,

    /// Full visual activity stack: YuNet + motion + size changes.
    /// Best quality for non-stereo audio sources.
    /// Uses motion and size changes with temporal smoothing.
    ActivityAware,
}

impl DetectionTier {
    /// All available detection tiers.
    pub const ALL: &'static [DetectionTier] = &[
        DetectionTier::None,
        DetectionTier::Basic,
        DetectionTier::AudioAware,
        DetectionTier::SpeakerAware,
        DetectionTier::MotionAware,
        DetectionTier::ActivityAware,
    ];

    /// Returns the tier name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectionTier::None => "none",
            DetectionTier::Basic => "basic",
            DetectionTier::AudioAware => "audio_aware",
            DetectionTier::SpeakerAware => "speaker_aware",
            DetectionTier::MotionAware => "motion_aware",
            DetectionTier::ActivityAware => "activity_aware",
        }
    }

    /// Returns a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            DetectionTier::None => "Heuristic positioning only (fastest)",
            DetectionTier::Basic => "YuNet face detection",
            DetectionTier::AudioAware => "Face detection + stereo audio activity",
            DetectionTier::SpeakerAware => "Full detection with speaker analysis",
            DetectionTier::MotionAware => "Face detection + visual motion",
            DetectionTier::ActivityAware => "Full visual activity tracking",
        }
    }

    /// Returns relative processing speed (1 = fastest, 6 = slowest).
    pub fn speed_rank(&self) -> u8 {
        match self {
            DetectionTier::None => 1,
            DetectionTier::Basic => 2,
            DetectionTier::AudioAware => 3,
            DetectionTier::MotionAware => 4,
            DetectionTier::SpeakerAware => 5,
            DetectionTier::ActivityAware => 6,
        }
    }

    /// Returns true if this tier requires YuNet face detection.
    pub fn requires_yunet(&self) -> bool {
        !matches!(self, DetectionTier::None)
    }

    /// Returns true if this tier uses stereo audio analysis.
    /// These tiers require stereo audio with speaker panning to work correctly.
    pub fn uses_audio(&self) -> bool {
        matches!(self, DetectionTier::AudioAware | DetectionTier::SpeakerAware)
    }

    /// Returns true if this tier uses visual motion/activity analysis.
    /// These tiers work with any audio format.
    pub fn uses_visual_activity(&self) -> bool {
        matches!(self, DetectionTier::MotionAware | DetectionTier::ActivityAware)
    }

    /// Returns true if this tier uses face activity analysis (temporal tracking).
    pub fn uses_face_activity(&self) -> bool {
        matches!(self, DetectionTier::SpeakerAware | DetectionTier::ActivityAware)
    }
}

impl fmt::Display for DetectionTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DetectionTier {
    type Err = DetectionTierParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(DetectionTier::None),
            "basic" => Ok(DetectionTier::Basic),
            "audio_aware" | "audio" => Ok(DetectionTier::AudioAware),
            "speaker_aware" | "speaker" => Ok(DetectionTier::SpeakerAware),
            "motion_aware" | "motion" => Ok(DetectionTier::MotionAware),
            "activity_aware" | "activity" => Ok(DetectionTier::ActivityAware),
            _ => Err(DetectionTierParseError(s.to_string())),
        }
    }
}

#[derive(Debug, Error)]
#[error("Unknown detection tier: {0}")]
pub struct DetectionTierParseError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_parse() {
        assert_eq!("none".parse::<DetectionTier>().unwrap(), DetectionTier::None);
        assert_eq!("basic".parse::<DetectionTier>().unwrap(), DetectionTier::Basic);
        assert_eq!("audio_aware".parse::<DetectionTier>().unwrap(), DetectionTier::AudioAware);
        assert_eq!("audio".parse::<DetectionTier>().unwrap(), DetectionTier::AudioAware);
        assert_eq!("speaker_aware".parse::<DetectionTier>().unwrap(), DetectionTier::SpeakerAware);
        assert_eq!("motion_aware".parse::<DetectionTier>().unwrap(), DetectionTier::MotionAware);
        assert_eq!("motion".parse::<DetectionTier>().unwrap(), DetectionTier::MotionAware);
        assert_eq!("activity_aware".parse::<DetectionTier>().unwrap(), DetectionTier::ActivityAware);
        assert_eq!("activity".parse::<DetectionTier>().unwrap(), DetectionTier::ActivityAware);
        assert!("invalid".parse::<DetectionTier>().is_err());
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(DetectionTier::None.to_string(), "none");
        assert_eq!(DetectionTier::SpeakerAware.to_string(), "speaker_aware");
        assert_eq!(DetectionTier::MotionAware.to_string(), "motion_aware");
        assert_eq!(DetectionTier::ActivityAware.to_string(), "activity_aware");
    }

    #[test]
    fn test_tier_requirements() {
        assert!(!DetectionTier::None.requires_yunet());
        assert!(DetectionTier::Basic.requires_yunet());
        assert!(DetectionTier::AudioAware.requires_yunet());
        assert!(DetectionTier::MotionAware.requires_yunet());

        // Audio-based tiers
        assert!(!DetectionTier::Basic.uses_audio());
        assert!(DetectionTier::AudioAware.uses_audio());
        assert!(DetectionTier::SpeakerAware.uses_audio());
        assert!(!DetectionTier::MotionAware.uses_audio());
        assert!(!DetectionTier::ActivityAware.uses_audio());

        // Visual-based tiers
        assert!(!DetectionTier::Basic.uses_visual_activity());
        assert!(!DetectionTier::AudioAware.uses_visual_activity());
        assert!(DetectionTier::MotionAware.uses_visual_activity());
        assert!(DetectionTier::ActivityAware.uses_visual_activity());

        // Face activity (temporal tracking)
        assert!(!DetectionTier::AudioAware.uses_face_activity());
        assert!(DetectionTier::SpeakerAware.uses_face_activity());
        assert!(!DetectionTier::MotionAware.uses_face_activity());
        assert!(DetectionTier::ActivityAware.uses_face_activity());
    }

    #[test]
    fn test_speed_ranking() {
        assert!(DetectionTier::None.speed_rank() < DetectionTier::Basic.speed_rank());
        assert!(DetectionTier::Basic.speed_rank() < DetectionTier::AudioAware.speed_rank());
        assert!(DetectionTier::AudioAware.speed_rank() < DetectionTier::MotionAware.speed_rank());
        assert!(DetectionTier::MotionAware.speed_rank() < DetectionTier::SpeakerAware.speed_rank());
        assert!(DetectionTier::SpeakerAware.speed_rank() < DetectionTier::ActivityAware.speed_rank());
    }
}

