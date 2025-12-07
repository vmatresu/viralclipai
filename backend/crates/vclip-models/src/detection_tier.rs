//! Detection tier definitions for intelligent video processing.
//!
//! This module defines the detection tiers that control which detection
//! providers are used during video processing:
//!
//! - `None`: Heuristic positioning only (fastest)
//! - `Basic`: YuNet face detection
//! - `AudioAware`: YuNet + audio activity detection
//! - `SpeakerAware`: YuNet + audio + face activity analysis

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Detection tier for intelligent video processing.
///
/// Controls which detection providers are used during processing.
/// Higher tiers provide better quality but require more processing time.
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

    /// YuNet face detection + audio activity detection.
    /// Uses audio energy to identify active speakers.
    AudioAware,

    /// Full detection stack: YuNet + audio + face activity.
    /// Best quality, uses mouth movement and motion analysis.
    SpeakerAware,
}

impl DetectionTier {
    /// All available detection tiers.
    pub const ALL: &'static [DetectionTier] = &[
        DetectionTier::None,
        DetectionTier::Basic,
        DetectionTier::AudioAware,
        DetectionTier::SpeakerAware,
    ];

    /// Returns the tier name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectionTier::None => "none",
            DetectionTier::Basic => "basic",
            DetectionTier::AudioAware => "audio_aware",
            DetectionTier::SpeakerAware => "speaker_aware",
        }
    }

    /// Returns a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            DetectionTier::None => "Heuristic positioning only (fastest)",
            DetectionTier::Basic => "YuNet face detection",
            DetectionTier::AudioAware => "Face detection + audio activity",
            DetectionTier::SpeakerAware => "Full detection with speaker analysis",
        }
    }

    /// Returns relative processing speed (1 = fastest, 4 = slowest).
    pub fn speed_rank(&self) -> u8 {
        match self {
            DetectionTier::None => 1,
            DetectionTier::Basic => 2,
            DetectionTier::AudioAware => 3,
            DetectionTier::SpeakerAware => 4,
        }
    }

    /// Returns true if this tier requires YuNet face detection.
    pub fn requires_yunet(&self) -> bool {
        !matches!(self, DetectionTier::None)
    }

    /// Returns true if this tier uses audio analysis.
    pub fn uses_audio(&self) -> bool {
        matches!(self, DetectionTier::AudioAware | DetectionTier::SpeakerAware)
    }

    /// Returns true if this tier uses face activity analysis.
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
            "audio_aware" | "audio" => Ok(DetectionTier::AudioAware),
            "speaker_aware" | "speaker" => Ok(DetectionTier::SpeakerAware),
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
        assert!("invalid".parse::<DetectionTier>().is_err());
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(DetectionTier::None.to_string(), "none");
        assert_eq!(DetectionTier::SpeakerAware.to_string(), "speaker_aware");
    }

    #[test]
    fn test_tier_requirements() {
        assert!(!DetectionTier::None.requires_yunet());
        assert!(DetectionTier::Basic.requires_yunet());
        assert!(DetectionTier::AudioAware.requires_yunet());

        assert!(!DetectionTier::Basic.uses_audio());
        assert!(DetectionTier::AudioAware.uses_audio());
        assert!(DetectionTier::SpeakerAware.uses_audio());

        assert!(!DetectionTier::AudioAware.uses_face_activity());
        assert!(DetectionTier::SpeakerAware.uses_face_activity());
    }

    #[test]
    fn test_speed_ranking() {
        assert!(DetectionTier::None.speed_rank() < DetectionTier::Basic.speed_rank());
        assert!(DetectionTier::Basic.speed_rank() < DetectionTier::AudioAware.speed_rank());
        assert!(DetectionTier::AudioAware.speed_rank() < DetectionTier::SpeakerAware.speed_rank());
    }
}
