//! Video style and crop mode definitions.

use crate::detection_tier::DetectionTier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Available clip styles.
///
/// Styles are organized into categories:
/// - **Static styles**: `Split`, `LeftFocus`, `RightFocus`, `Original` - fixed crops
/// - **Fast styles**: `SplitFast` - heuristic positioning without AI
/// - **Intelligent styles**: Use AI detection with varying tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Original aspect ratio preserved
    Original,
    /// Split view - left and right halves stacked (static crop)
    Split,
    /// Focus on left half (static crop)
    LeftFocus,
    /// Focus on right half (static crop)
    RightFocus,
    /// Fast split view - heuristic positioning only, no AI detection
    SplitFast,
    /// Intelligent crop with face tracking (single view, basic tier)
    Intelligent,
    /// Intelligent split with face tracking (dual view, basic tier)
    IntelligentSplit,
    /// Intelligent crop - YuNet face detection only
    IntelligentBasic,
    /// Intelligent split - YuNet face detection only
    IntelligentSplitBasic,
    /// Intelligent crop - YuNet + audio activity detection (requires stereo audio)
    IntelligentAudio,
    /// Intelligent split - YuNet + audio activity detection (requires stereo audio)
    IntelligentSplitAudio,
    /// Intelligent crop - full detection (YuNet + audio + face activity, requires stereo audio)
    IntelligentSpeaker,
    /// Intelligent split - full detection (YuNet + audio + face activity, requires stereo audio)
    IntelligentSplitSpeaker,
    /// Intelligent crop - YuNet + visual motion detection (works with any audio)
    IntelligentMotion,
    /// Intelligent split - YuNet + visual motion detection (works with any audio)
    IntelligentSplitMotion,
    /// Intelligent crop - full visual activity (motion + size + temporal, works with any audio)
    IntelligentActivity,
    /// Intelligent split - full visual activity (motion + size + temporal, works with any audio)
    IntelligentSplitActivity,
}

impl Style {
    /// All available styles.
    pub const ALL: &'static [Style] = &[
        Style::Original,
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
        Style::SplitFast,
        Style::Intelligent,
        Style::IntelligentSplit,
        Style::IntelligentBasic,
        Style::IntelligentSplitBasic,
        Style::IntelligentAudio,
        Style::IntelligentSplitAudio,
        Style::IntelligentSpeaker,
        Style::IntelligentSplitSpeaker,
        Style::IntelligentMotion,
        Style::IntelligentSplitMotion,
        Style::IntelligentActivity,
        Style::IntelligentSplitActivity,
    ];

    /// Styles included when user requests "all".
    /// Excludes Original and advanced tiers to avoid overwhelming output.
    pub const ALL_FOR_EXPANSION: &'static [Style] = &[
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
        Style::SplitFast,
        Style::Intelligent,
        Style::IntelligentSplit,
    ];

    /// Expand a list of style strings, handling "all" keyword.
    /// Returns None for invalid styles (they are filtered out).
    pub fn expand_styles(style_strs: &[String]) -> Vec<Style> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for s in style_strs {
            let lower = s.to_lowercase();
            if lower == "all" {
                // Expand "all" to all available styles
                for style in Self::ALL_FOR_EXPANSION {
                    if seen.insert(*style) {
                        result.push(*style);
                    }
                }
            } else if let Ok(style) = lower.parse::<Style>() {
                if seen.insert(style) {
                    result.push(style);
                }
            }
            // Invalid styles are silently filtered out
        }

        result
    }

    /// Returns the style name as used in filenames.
    pub fn as_filename_part(&self) -> &'static str {
        match self {
            Style::Original => "original",
            Style::Split => "split",
            Style::LeftFocus => "left_focus",
            Style::RightFocus => "right_focus",
            Style::SplitFast => "split_fast",
            Style::Intelligent => "intelligent",
            Style::IntelligentSplit => "intelligent_split",
            Style::IntelligentBasic => "intelligent_basic",
            Style::IntelligentSplitBasic => "intelligent_split_basic",
            Style::IntelligentAudio => "intelligent_audio",
            Style::IntelligentSplitAudio => "intelligent_split_audio",
            Style::IntelligentSpeaker => "intelligent_speaker",
            Style::IntelligentSplitSpeaker => "intelligent_split_speaker",
            Style::IntelligentMotion => "intelligent_motion",
            Style::IntelligentSplitMotion => "intelligent_split_motion",
            Style::IntelligentActivity => "intelligent_activity",
            Style::IntelligentSplitActivity => "intelligent_split_activity",
        }
    }

    /// Whether this style requires intelligent cropping.
    pub fn requires_intelligent_crop(&self) -> bool {
        matches!(
            self,
            Style::Intelligent
                | Style::IntelligentSplit
                | Style::IntelligentBasic
                | Style::IntelligentSplitBasic
                | Style::IntelligentAudio
                | Style::IntelligentSplitAudio
                | Style::IntelligentSpeaker
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentMotion
                | Style::IntelligentSplitMotion
                | Style::IntelligentActivity
                | Style::IntelligentSplitActivity
        )
    }

    /// Returns the detection tier for this style.
    pub fn detection_tier(&self) -> DetectionTier {
        match self {
            Style::Original
            | Style::Split
            | Style::LeftFocus
            | Style::RightFocus
            | Style::SplitFast => DetectionTier::None,
            Style::Intelligent
            | Style::IntelligentSplit
            | Style::IntelligentBasic
            | Style::IntelligentSplitBasic => DetectionTier::Basic,
            Style::IntelligentAudio | Style::IntelligentSplitAudio => DetectionTier::AudioAware,
            Style::IntelligentSpeaker | Style::IntelligentSplitSpeaker => DetectionTier::SpeakerAware,
            Style::IntelligentMotion | Style::IntelligentSplitMotion => DetectionTier::MotionAware,
            Style::IntelligentActivity | Style::IntelligentSplitActivity => DetectionTier::ActivityAware,
        }
    }

    /// Credits required to generate a single clip in this style.
    ///
    /// Kept at 1 for all styles today, but structured for future per-style pricing.
    pub fn credit_cost(&self) -> u32 {
        1
    }

    /// Returns true if this is a split-view style.
    pub fn is_split_view(&self) -> bool {
        matches!(
            self,
            Style::Split
                | Style::SplitFast
                | Style::IntelligentSplit
                | Style::IntelligentSplitBasic
                | Style::IntelligentSplitAudio
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentSplitMotion
                | Style::IntelligentSplitActivity
        )
    }

    /// Returns true if this is a fast/static style (no AI detection).
    pub fn is_fast(&self) -> bool {
        self.detection_tier() == DetectionTier::None
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_filename_part())
    }
}

impl FromStr for Style {
    type Err = StyleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "original" => Ok(Style::Original),
            "split" => Ok(Style::Split),
            "left_focus" => Ok(Style::LeftFocus),
            "right_focus" => Ok(Style::RightFocus),
            "split_fast" => Ok(Style::SplitFast),
            "intelligent" => Ok(Style::Intelligent),
            "intelligent_split" => Ok(Style::IntelligentSplit),
            "intelligent_basic" => Ok(Style::IntelligentBasic),
            "intelligent_split_basic" => Ok(Style::IntelligentSplitBasic),
            "intelligent_audio" => Ok(Style::IntelligentAudio),
            "intelligent_split_audio" => Ok(Style::IntelligentSplitAudio),
            "intelligent_speaker" => Ok(Style::IntelligentSpeaker),
            "intelligent_split_speaker" => Ok(Style::IntelligentSplitSpeaker),
            "intelligent_motion" => Ok(Style::IntelligentMotion),
            "intelligent_split_motion" => Ok(Style::IntelligentSplitMotion),
            "intelligent_activity" => Ok(Style::IntelligentActivity),
            "intelligent_split_activity" => Ok(Style::IntelligentSplitActivity),
            _ => Err(StyleParseError(s.to_string())),
        }
    }
}

#[derive(Debug, Error)]
#[error("Unknown style: {0}")]
pub struct StyleParseError(String);

/// Crop mode for video processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CropMode {
    /// No cropping
    #[default]
    None,
    /// Center crop
    Center,
    /// Manual crop (user-defined)
    Manual,
    /// Intelligent crop with face tracking
    Intelligent,
}

impl CropMode {
    pub const ALL: &'static [CropMode] = &[
        CropMode::None,
        CropMode::Center,
        CropMode::Manual,
        CropMode::Intelligent,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CropMode::None => "none",
            CropMode::Center => "center",
            CropMode::Manual => "manual",
            CropMode::Intelligent => "intelligent",
        }
    }
}

impl fmt::Display for CropMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CropMode {
    type Err = CropModeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(CropMode::None),
            "center" => Ok(CropMode::Center),
            "manual" => Ok(CropMode::Manual),
            "intelligent" => Ok(CropMode::Intelligent),
            _ => Err(CropModeParseError(s.to_string())),
        }
    }
}

#[derive(Debug, Error)]
#[error("Unknown crop mode: {0}")]
pub struct CropModeParseError(String);

/// Aspect ratio specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct AspectRatio {
    pub width: u32,
    pub height: u32,
}

impl AspectRatio {
    /// Standard portrait (9:16) for TikTok/Reels
    pub const PORTRAIT: AspectRatio = AspectRatio {
        width: 9,
        height: 16,
    };

    /// Square (1:1)
    pub const SQUARE: AspectRatio = AspectRatio {
        width: 1,
        height: 1,
    };

    /// Instagram portrait (4:5)
    pub const INSTAGRAM_PORTRAIT: AspectRatio = AspectRatio {
        width: 4,
        height: 5,
    };

    /// Split view aspect (9:8)
    pub const SPLIT_VIEW: AspectRatio = AspectRatio {
        width: 9,
        height: 8,
    };

    /// Create a new aspect ratio.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Returns the aspect ratio as a decimal.
    pub fn as_f64(&self) -> f64 {
        self.width as f64 / self.height as f64
    }
}

impl fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.width, self.height)
    }
}

impl FromStr for AspectRatio {
    type Err = AspectRatioParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(AspectRatioParseError::InvalidFormat(s.to_string()));
        }

        let width = parts[0]
            .parse()
            .map_err(|_| AspectRatioParseError::InvalidNumber(parts[0].to_string()))?;
        let height = parts[1]
            .parse()
            .map_err(|_| AspectRatioParseError::InvalidNumber(parts[1].to_string()))?;

        if width == 0 || height == 0 {
            return Err(AspectRatioParseError::ZeroValue);
        }

        Ok(AspectRatio { width, height })
    }
}

impl Default for AspectRatio {
    fn default() -> Self {
        Self::PORTRAIT
    }
}

#[derive(Debug, Error)]
pub enum AspectRatioParseError {
    #[error("Invalid aspect ratio format: {0}, expected 'W:H'")]
    InvalidFormat(String),
    #[error("Invalid number in aspect ratio: {0}")]
    InvalidNumber(String),
    #[error("Aspect ratio cannot have zero values")]
    ZeroValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_parse() {
        assert_eq!("split".parse::<Style>().unwrap(), Style::Split);
        assert_eq!(
            "intelligent_split".parse::<Style>().unwrap(),
            Style::IntelligentSplit
        );
        assert!("unknown".parse::<Style>().is_err());
    }

    #[test]
    fn test_intelligent_styles() {
        // "intelligent" is its own style
        assert_eq!(
            "intelligent".parse::<Style>().unwrap(),
            Style::Intelligent
        );
        assert_eq!(
            "INTELLIGENT".parse::<Style>().unwrap(),
            Style::Intelligent
        );
        // "intelligent_split" is a separate style
        assert_eq!(
            "intelligent_split".parse::<Style>().unwrap(),
            Style::IntelligentSplit
        );
    }

    #[test]
    fn test_expand_styles_all() {
        let styles = Style::expand_styles(&["all".to_string()]);
        assert_eq!(styles.len(), 6); // Now includes SplitFast
        assert!(styles.contains(&Style::Split));
        assert!(styles.contains(&Style::SplitFast));
        assert!(styles.contains(&Style::LeftFocus));
        assert!(styles.contains(&Style::RightFocus));
        assert!(styles.contains(&Style::Intelligent));
        assert!(styles.contains(&Style::IntelligentSplit));
        // "all" should not include Original or advanced tiers
        assert!(!styles.contains(&Style::Original));
        assert!(!styles.contains(&Style::IntelligentSpeaker));
    }

    #[test]
    fn test_expand_styles_mixed() {
        let styles = Style::expand_styles(&[
            "split".to_string(),
            "original".to_string(),
            "invalid".to_string(),
        ]);
        assert_eq!(styles.len(), 2);
        assert!(styles.contains(&Style::Split));
        assert!(styles.contains(&Style::Original));
    }

    #[test]
    fn test_expand_styles_dedup() {
        let styles = Style::expand_styles(&[
            "split".to_string(),
            "all".to_string(),
            "split".to_string(),
        ]);
        // Should deduplicate: split appears once, all expands but split already seen
        assert_eq!(styles.len(), 6);
    }

    #[test]
    fn test_aspect_ratio_parse() {
        assert_eq!(
            "9:16".parse::<AspectRatio>().unwrap(),
            AspectRatio::PORTRAIT
        );
        assert_eq!("1:1".parse::<AspectRatio>().unwrap(), AspectRatio::SQUARE);
        assert!("invalid".parse::<AspectRatio>().is_err());
        assert!("0:16".parse::<AspectRatio>().is_err());
    }

    #[test]
    fn test_style_display() {
        assert_eq!(Style::IntelligentSplit.to_string(), "intelligent_split");
    }

    #[test]
    fn test_style_credit_cost_defaults_to_one() {
        for style in Style::ALL.iter() {
            assert_eq!(style.credit_cost(), 1, "unexpected credit cost for {:?}", style);
        }
    }

    #[test]
    fn test_detection_tier_mapping() {
        use crate::detection_tier::DetectionTier;
        
        // Fast styles -> None tier
        assert_eq!(Style::Original.detection_tier(), DetectionTier::None);
        assert_eq!(Style::Split.detection_tier(), DetectionTier::None);
        assert_eq!(Style::SplitFast.detection_tier(), DetectionTier::None);
        
        // Basic tier styles
        assert_eq!(Style::Intelligent.detection_tier(), DetectionTier::Basic);
        assert_eq!(Style::IntelligentSplit.detection_tier(), DetectionTier::Basic);
        assert_eq!(Style::IntelligentBasic.detection_tier(), DetectionTier::Basic);
        
        // Audio-aware tier
        assert_eq!(Style::IntelligentAudio.detection_tier(), DetectionTier::AudioAware);
        assert_eq!(Style::IntelligentSplitAudio.detection_tier(), DetectionTier::AudioAware);
        
        // Speaker-aware tier
        assert_eq!(Style::IntelligentSpeaker.detection_tier(), DetectionTier::SpeakerAware);
        assert_eq!(Style::IntelligentSplitSpeaker.detection_tier(), DetectionTier::SpeakerAware);
    }

    #[test]
    fn test_is_split_view() {
        // Split view styles
        assert!(Style::Split.is_split_view());
        assert!(Style::SplitFast.is_split_view());
        assert!(Style::IntelligentSplit.is_split_view());
        assert!(Style::IntelligentSplitAudio.is_split_view());
        
        // Non-split styles
        assert!(!Style::Original.is_split_view());
        assert!(!Style::Intelligent.is_split_view());
        assert!(!Style::IntelligentSpeaker.is_split_view());
    }

    #[test]
    fn test_is_fast() {
        // Fast styles (no AI detection)
        assert!(Style::Original.is_fast());
        assert!(Style::Split.is_fast());
        assert!(Style::SplitFast.is_fast());
        assert!(Style::LeftFocus.is_fast());
        
        // Non-fast styles (use AI detection)
        assert!(!Style::Intelligent.is_fast());
        assert!(!Style::IntelligentSpeaker.is_fast());
    }

    #[test]
    fn test_new_style_parse() {
        assert_eq!("split_fast".parse::<Style>().unwrap(), Style::SplitFast);
        assert_eq!("intelligent_audio".parse::<Style>().unwrap(), Style::IntelligentAudio);
        assert_eq!("intelligent_speaker".parse::<Style>().unwrap(), Style::IntelligentSpeaker);
        assert_eq!("intelligent_split_audio".parse::<Style>().unwrap(), Style::IntelligentSplitAudio);
        assert_eq!("intelligent_split_speaker".parse::<Style>().unwrap(), Style::IntelligentSplitSpeaker);
    }
}

