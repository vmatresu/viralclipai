//! Detection tier definitions for intelligent video processing.
//!
//! This module defines the detection tiers that control which detection
//! providers are used during video processing:
//!
//! - `None`: Heuristic positioning only (fastest)
//! - `Basic`: YuNet face detection
//! - `SpeakerAware`: YuNet + face mesh mouth activity (visual-only)
//! - `MotionAware`: Visual motion heuristics (no NN, no audio)

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
/// Audio-based tiers were removed; all tiers are now visual-only.
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

    /// Full detection stack: YuNet + face mesh mouth activity (visual-only).
    /// Best quality for multi-speaker content.
    SpeakerAware,

    /// Visual motion detection (heuristic, no NN).
    /// Uses frame differencing to detect active regions.
    MotionAware,
}

impl DetectionTier {
    /// All available detection tiers.
    pub const ALL: &'static [DetectionTier] = &[
        DetectionTier::None,
        DetectionTier::Basic,
        DetectionTier::SpeakerAware,
        DetectionTier::MotionAware,
    ];

    /// Returns the tier name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectionTier::None => "none",
            DetectionTier::Basic => "basic",
            DetectionTier::SpeakerAware => "speaker_aware",
            DetectionTier::MotionAware => "motion_aware",
        }
    }

    /// Returns a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            DetectionTier::None => "Heuristic positioning only (fastest)",
            DetectionTier::Basic => "YuNet face detection",
            DetectionTier::SpeakerAware => "YuNet + face mesh mouth activity (visual-only)",
            DetectionTier::MotionAware => "Visual motion heuristics (no NN)",
        }
    }

    /// Returns relative processing speed (1 = fastest, 6 = slowest).
    pub fn speed_rank(&self) -> u8 {
        match self {
            DetectionTier::None => 1,
            DetectionTier::Basic => 2,
            DetectionTier::MotionAware => 3,
            DetectionTier::SpeakerAware => 4,
        }
    }

    /// Returns true if this tier requires YuNet face detection.
    pub fn requires_yunet(&self) -> bool {
        matches!(self, DetectionTier::Basic | DetectionTier::SpeakerAware)
    }

    /// Returns true if this tier uses stereo audio analysis.
    /// These tiers require stereo audio with speaker panning to work correctly.
    pub fn uses_audio(&self) -> bool {
        false
    }

    /// Returns true if this tier uses visual motion/activity analysis.
    /// These tiers work without audio.
    pub fn uses_visual_activity(&self) -> bool {
        matches!(self, DetectionTier::MotionAware | DetectionTier::SpeakerAware)
    }

    /// Returns true if this tier uses face activity analysis (temporal tracking).
    pub fn uses_face_activity(&self) -> bool {
        matches!(self, DetectionTier::SpeakerAware)
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
            "speaker_aware" | "speaker" => Ok(DetectionTier::SpeakerAware),
            "motion_aware" | "motion" => Ok(DetectionTier::MotionAware),
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
        assert_eq!("speaker_aware".parse::<DetectionTier>().unwrap(), DetectionTier::SpeakerAware);
        assert_eq!("motion_aware".parse::<DetectionTier>().unwrap(), DetectionTier::MotionAware);
        assert_eq!("motion".parse::<DetectionTier>().unwrap(), DetectionTier::MotionAware);
        assert!("invalid".parse::<DetectionTier>().is_err());
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(DetectionTier::None.to_string(), "none");
        assert_eq!(DetectionTier::SpeakerAware.to_string(), "speaker_aware");
        assert_eq!(DetectionTier::MotionAware.to_string(), "motion_aware");
    }

    #[test]
    fn test_tier_requirements() {
        assert!(!DetectionTier::None.requires_yunet());
        assert!(DetectionTier::Basic.requires_yunet());
        assert!(!DetectionTier::MotionAware.requires_yunet());

        // Audio is disabled for all tiers
        assert!(!DetectionTier::None.uses_audio());
        assert!(!DetectionTier::Basic.uses_audio());
        assert!(!DetectionTier::SpeakerAware.uses_audio());
        assert!(!DetectionTier::MotionAware.uses_audio());

        // Visual activity
        assert!(!DetectionTier::Basic.uses_visual_activity());
        assert!(DetectionTier::MotionAware.uses_visual_activity());
        assert!(DetectionTier::SpeakerAware.uses_visual_activity());

        // Face activity (temporal tracking)
        assert!(DetectionTier::SpeakerAware.uses_face_activity());
        assert!(!DetectionTier::MotionAware.uses_face_activity());
    }

    #[test]
    fn test_speed_ranking() {
        assert!(DetectionTier::None.speed_rank() < DetectionTier::Basic.speed_rank());
        assert!(DetectionTier::Basic.speed_rank() < DetectionTier::MotionAware.speed_rank());
        assert!(DetectionTier::MotionAware.speed_rank() < DetectionTier::SpeakerAware.speed_rank());
    }
}

