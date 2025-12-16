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
/// - **Static styles**: `Split`, `LeftFocus`, `CenterFocus`, `RightFocus`, `Original` - fixed crops
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
    /// Focus on center vertical slice (static crop)
    CenterFocus,
    /// Fast split view - heuristic positioning only, no AI detection
    SplitFast,
    /// Intelligent crop with face tracking (single view, basic tier)
    Intelligent,
    /// Intelligent split with face tracking (dual view, basic tier)
    IntelligentSplit,
    /// Intelligent crop - full detection (YuNet + face mesh activity, visual-only)
    IntelligentSpeaker,
    /// Intelligent split - full detection (YuNet + face mesh activity, visual-only)
    IntelligentSplitSpeaker,
    /// Intelligent crop - visual motion heuristic (no NN)
    IntelligentMotion,
    /// Intelligent split - visual motion heuristic (no NN)
    IntelligentSplitMotion,
    /// Intelligent split - dynamic activity-based split (2+ speakers = split)
    IntelligentSplitActivity,
    /// Intelligent cinematic - AutoAI-inspired smooth camera with polynomial trajectory
    IntelligentCinematic,
    /// Streamer split - original gameplay on top, face cam on bottom (for gaming/explainer content)
    StreamerSplit,
    /// Streamer (full view) - landscape video centered with blurred portrait background (no AI)
    Streamer,
    /// Streamer Top Scenes - compilation of selected scenes with countdown overlay (no AI)
    StreamerTopScenes,
}

impl Style {
    /// All available styles.
    pub const ALL: &'static [Style] = &[
        Style::Original,
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
        Style::CenterFocus,
        Style::SplitFast,
        Style::Intelligent,
        Style::IntelligentSplit,
        Style::IntelligentSpeaker,
        Style::IntelligentSplitSpeaker,
        Style::IntelligentMotion,
        Style::IntelligentSplitMotion,
        Style::IntelligentSplitActivity,
        Style::IntelligentCinematic,
        Style::StreamerSplit,
        Style::Streamer,
        Style::StreamerTopScenes,
    ];

    /// Styles included when user requests "all".
    /// Excludes Original and advanced tiers to avoid overwhelming output.
    pub const ALL_FOR_EXPANSION: &'static [Style] = &[
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
        Style::CenterFocus,
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
            Style::CenterFocus => "center_focus",
            Style::SplitFast => "split_fast",
            Style::Intelligent => "intelligent",
            Style::IntelligentSplit => "intelligent_split",
            Style::IntelligentSpeaker => "intelligent_speaker",
            Style::IntelligentSplitSpeaker => "intelligent_split_speaker",
            Style::IntelligentMotion => "intelligent_motion",
            Style::IntelligentSplitMotion => "intelligent_split_motion",
            Style::IntelligentSplitActivity => "intelligent_split_activity",
            Style::IntelligentCinematic => "intelligent_cinematic",
            Style::StreamerSplit => "streamer_split",
            Style::Streamer => "streamer",
            Style::StreamerTopScenes => "streamer_top_scenes",
        }
    }

    /// Whether this style requires intelligent cropping.
    pub fn requires_intelligent_crop(&self) -> bool {
        matches!(
            self,
            Style::Intelligent
                | Style::IntelligentSplit
                | Style::IntelligentSpeaker
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentMotion
                | Style::IntelligentSplitMotion
                | Style::IntelligentSplitActivity
                | Style::IntelligentCinematic
            // Note: StreamerSplit uses user-specified params, not intelligent cropping
        )
    }

    /// Whether this style requires face detection (can benefit from neural cache).
    ///
    /// Returns true for styles that use YuNet/FaceMesh detection.
    /// MotionAware styles use motion heuristics instead of face detection.
    /// StreamerSplit uses user-specified params, no face detection needed.
    pub fn requires_face_detection(&self) -> bool {
        matches!(
            self,
            Style::Intelligent
                | Style::IntelligentSplit
                | Style::IntelligentSpeaker
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentSplitActivity
                | Style::IntelligentCinematic
            // Note: StreamerSplit removed - uses user-specified crop params
        )
    }

    /// Whether this style can benefit from cached analysis (face detection OR motion heuristics).
    ///
    /// Returns true for all intelligent styles that perform expensive per-frame analysis.
    /// This includes both face detection styles and motion-aware styles.
    ///
    /// Note: This only indicates the style CAN USE cache if available.
    /// Use `should_generate_cached_analysis()` to check if this style should TRIGGER cache generation.
    /// StreamerSplit uses user-specified params, no cache needed.
    pub fn can_use_cached_analysis(&self) -> bool {
        matches!(
            self,
            Style::Intelligent
                | Style::IntelligentSplit
                | Style::IntelligentSpeaker
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentSplitActivity
                | Style::IntelligentMotion
                | Style::IntelligentSplitMotion
                | Style::IntelligentCinematic
            // Note: StreamerSplit removed - uses user-specified crop params
        )
    }

    /// Whether this style should trigger neural analysis cache generation.
    ///
    /// Only premium tiers (SpeakerAware, MotionAware, Cinematic) should generate and cache analysis.
    /// These are gated to Pro/Studio plans. Lower tiers (Basic) can consume cached
    /// analysis if it exists, but should never trigger expensive cache generation.
    /// StreamerSplit uses user-specified params, no cache generation needed.
    pub fn should_generate_cached_analysis(&self) -> bool {
        matches!(
            self,
            Style::IntelligentSpeaker
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentMotion
                | Style::IntelligentSplitMotion
                | Style::IntelligentSplitActivity
                | Style::IntelligentCinematic
            // Note: StreamerSplit removed - uses user-specified crop params
        )
    }

    /// Returns the detection tier for this style.
    pub fn detection_tier(&self) -> DetectionTier {
        match self {
            Style::Original
            | Style::Split
            | Style::LeftFocus
            | Style::RightFocus
            | Style::CenterFocus
            | Style::SplitFast => DetectionTier::None,
            Style::Intelligent | Style::IntelligentSplit => DetectionTier::Basic,
            Style::IntelligentSpeaker | Style::IntelligentSplitSpeaker => {
                DetectionTier::SpeakerAware
            }
            Style::IntelligentMotion | Style::IntelligentSplitMotion => DetectionTier::MotionAware,
            // Activity split works across tiers but usually requires at least Basic or SpeakerAware signals.
            // The tier is typically injected or determined at runtime, but here we can default to SpeakerAware
            // as it relies on activity signals.
            Style::IntelligentSplitActivity => DetectionTier::SpeakerAware,
            // Cinematic tier uses polynomial trajectory optimization + adaptive zoom
            Style::IntelligentCinematic => DetectionTier::Cinematic,
            // StreamerSplit uses user-specified params, no detection needed (fast)
            Style::StreamerSplit => DetectionTier::None,
            // Streamer styles are fast (no AI detection)
            Style::Streamer | Style::StreamerTopScenes => DetectionTier::None,
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
                | Style::IntelligentSplitSpeaker
                | Style::IntelligentSplitMotion
                | Style::IntelligentSplitActivity
                | Style::StreamerSplit
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
            "center_focus" => Ok(Style::CenterFocus),
            "split_fast" => Ok(Style::SplitFast),
            "intelligent" => Ok(Style::Intelligent),
            "intelligent_split" => Ok(Style::IntelligentSplit),
            "intelligent_speaker" => Ok(Style::IntelligentSpeaker),
            "intelligent_split_speaker" => Ok(Style::IntelligentSplitSpeaker),
            "intelligent_motion" => Ok(Style::IntelligentMotion),
            "intelligent_split_motion" => Ok(Style::IntelligentSplitMotion),
            "intelligent_split_activity" => Ok(Style::IntelligentSplitActivity),
            "intelligent_cinematic" | "cinematic" => Ok(Style::IntelligentCinematic),
            "streamer_split" => Ok(Style::StreamerSplit),
            "streamer" => Ok(Style::Streamer),
            "streamer_top_scenes" => Ok(Style::StreamerTopScenes),
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
        assert_eq!("center_focus".parse::<Style>().unwrap(), Style::CenterFocus);
        assert!("unknown".parse::<Style>().is_err());
    }

    #[test]
    fn test_intelligent_styles() {
        // "intelligent" is its own style
        assert_eq!("intelligent".parse::<Style>().unwrap(), Style::Intelligent);
        assert_eq!("INTELLIGENT".parse::<Style>().unwrap(), Style::Intelligent);
        // "intelligent_split" is a separate style
        assert_eq!(
            "intelligent_split".parse::<Style>().unwrap(),
            Style::IntelligentSplit
        );
    }

    #[test]
    fn test_expand_styles_all() {
        let styles = Style::expand_styles(&["all".to_string()]);
        assert_eq!(styles.len(), 7); // Includes center focus
        assert!(styles.contains(&Style::Split));
        assert!(styles.contains(&Style::SplitFast));
        assert!(styles.contains(&Style::LeftFocus));
        assert!(styles.contains(&Style::RightFocus));
        assert!(styles.contains(&Style::CenterFocus));
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
        let styles =
            Style::expand_styles(&["split".to_string(), "all".to_string(), "split".to_string()]);
        // Should deduplicate: split appears once, all expands but split already seen
        assert_eq!(styles.len(), 7);
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
            assert_eq!(
                style.credit_cost(),
                1,
                "unexpected credit cost for {:?}",
                style
            );
        }
    }

    #[test]
    fn test_detection_tier_mapping() {
        use crate::detection_tier::DetectionTier;

        // Fast styles -> None tier
        assert_eq!(Style::Original.detection_tier(), DetectionTier::None);
        assert_eq!(Style::Split.detection_tier(), DetectionTier::None);
        assert_eq!(Style::LeftFocus.detection_tier(), DetectionTier::None);
        assert_eq!(Style::RightFocus.detection_tier(), DetectionTier::None);
        assert_eq!(Style::CenterFocus.detection_tier(), DetectionTier::None);
        assert_eq!(Style::SplitFast.detection_tier(), DetectionTier::None);

        // Basic tier styles
        assert_eq!(Style::Intelligent.detection_tier(), DetectionTier::Basic);
        assert_eq!(
            Style::IntelligentSplit.detection_tier(),
            DetectionTier::Basic
        );
        // Speaker-aware tier
        assert_eq!(
            Style::IntelligentSpeaker.detection_tier(),
            DetectionTier::SpeakerAware
        );
        assert_eq!(
            Style::IntelligentSplitSpeaker.detection_tier(),
            DetectionTier::SpeakerAware
        );

        // Motion-aware tier
        assert_eq!(
            Style::IntelligentMotion.detection_tier(),
            DetectionTier::MotionAware
        );
        assert_eq!(
            Style::IntelligentSplitMotion.detection_tier(),
            DetectionTier::MotionAware
        );
    }

    #[test]
    fn test_is_split_view() {
        // Split view styles
        assert!(Style::Split.is_split_view());
        assert!(Style::SplitFast.is_split_view());
        assert!(Style::IntelligentSplit.is_split_view());
        assert!(Style::IntelligentSplitSpeaker.is_split_view());
        assert!(Style::IntelligentSplitMotion.is_split_view());

        // Non-split styles
        assert!(!Style::Original.is_split_view());
        assert!(!Style::Intelligent.is_split_view());
        assert!(!Style::IntelligentSpeaker.is_split_view());
        assert!(!Style::CenterFocus.is_split_view());
    }

    #[test]
    fn test_is_fast() {
        // Fast styles (no AI detection)
        assert!(Style::Original.is_fast());
        assert!(Style::Split.is_fast());
        assert!(Style::SplitFast.is_fast());
        assert!(Style::LeftFocus.is_fast());
        assert!(Style::RightFocus.is_fast());
        assert!(Style::CenterFocus.is_fast());

        // Non-fast styles (use AI detection)
        assert!(!Style::Intelligent.is_fast());
        assert!(!Style::IntelligentSpeaker.is_fast());
    }

    #[test]
    fn test_new_style_parse() {
        assert_eq!("split_fast".parse::<Style>().unwrap(), Style::SplitFast);
        assert_eq!(
            "intelligent_speaker".parse::<Style>().unwrap(),
            Style::IntelligentSpeaker
        );
        assert_eq!(
            "intelligent_split_speaker".parse::<Style>().unwrap(),
            Style::IntelligentSplitSpeaker
        );
        assert_eq!("center_focus".parse::<Style>().unwrap(), Style::CenterFocus);
        // Cinematic style supports both full name and shorthand
        assert_eq!(
            "intelligent_cinematic".parse::<Style>().unwrap(),
            Style::IntelligentCinematic
        );
        assert_eq!(
            "cinematic".parse::<Style>().unwrap(),
            Style::IntelligentCinematic
        );
    }

    #[test]
    fn test_cache_generation_gating() {
        // Premium tiers (Pro/Studio) should generate cache
        assert!(Style::IntelligentSpeaker.should_generate_cached_analysis());
        assert!(Style::IntelligentSplitSpeaker.should_generate_cached_analysis());
        assert!(Style::IntelligentMotion.should_generate_cached_analysis());
        assert!(Style::IntelligentSplitMotion.should_generate_cached_analysis());
        assert!(Style::IntelligentSplitActivity.should_generate_cached_analysis());
        assert!(Style::IntelligentCinematic.should_generate_cached_analysis());

        // Lower tiers should NOT generate cache (but can consume if available)
        assert!(!Style::Intelligent.should_generate_cached_analysis());
        assert!(!Style::IntelligentSplit.should_generate_cached_analysis());

        // StreamerSplit uses user params, no cache needed
        assert!(!Style::StreamerSplit.should_generate_cached_analysis());
        assert!(!Style::StreamerSplit.can_use_cached_analysis());

        // All intelligent styles can USE cache if available
        assert!(Style::Intelligent.can_use_cached_analysis());
        assert!(Style::IntelligentSplit.can_use_cached_analysis());
        assert!(Style::IntelligentSpeaker.can_use_cached_analysis());
        assert!(Style::IntelligentMotion.can_use_cached_analysis());
        assert!(Style::IntelligentCinematic.can_use_cached_analysis());

        // Non-intelligent styles cannot use cache
        assert!(!Style::Split.can_use_cached_analysis());
        assert!(!Style::Original.can_use_cached_analysis());
    }

    #[test]
    fn test_streamer_split_is_fast() {
        use crate::detection_tier::DetectionTier;

        // StreamerSplit is now a fast style (no AI detection)
        assert!(Style::StreamerSplit.is_fast());
        assert!(Style::StreamerSplit.is_split_view());
        assert_eq!(Style::StreamerSplit.detection_tier(), DetectionTier::None);
        assert!(!Style::StreamerSplit.requires_face_detection());
        assert!(!Style::StreamerSplit.requires_intelligent_crop());
    }

    #[test]
    fn test_cinematic_tier_mapping() {
        use crate::detection_tier::DetectionTier;

        assert_eq!(
            Style::IntelligentCinematic.detection_tier(),
            DetectionTier::Cinematic
        );
        assert!(Style::IntelligentCinematic.requires_intelligent_crop());
        assert!(Style::IntelligentCinematic.requires_face_detection());
        assert!(!Style::IntelligentCinematic.is_split_view());
        assert!(!Style::IntelligentCinematic.is_fast());
    }
}
