//! Video style and crop mode definitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Available clip styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Original aspect ratio preserved
    Original,
    /// Split view - left and right halves stacked
    Split,
    /// Focus on left half
    LeftFocus,
    /// Focus on right half
    RightFocus,
    /// Intelligent crop with face tracking (single view)
    Intelligent,
    /// Intelligent split with face tracking (dual view)
    IntelligentSplit,
}

impl Style {
    /// All available styles (excluding Original for "all" expansion).
    pub const ALL: &'static [Style] = &[
        Style::Original,
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
        Style::Intelligent,
        Style::IntelligentSplit,
    ];

    /// Styles included when user requests "all".
    /// Excludes Original as it's typically not wanted in batch processing.
    pub const ALL_FOR_EXPANSION: &'static [Style] = &[
        Style::Split,
        Style::LeftFocus,
        Style::RightFocus,
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
            Style::Intelligent => "intelligent",
            Style::IntelligentSplit => "intelligent_split",
        }
    }

    /// Whether this style requires intelligent cropping.
    pub fn requires_intelligent_crop(&self) -> bool {
        matches!(self, Style::Intelligent | Style::IntelligentSplit)
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
            "intelligent" => Ok(Style::Intelligent),
            "intelligent_split" => Ok(Style::IntelligentSplit),
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
        assert_eq!(styles.len(), 5);
        assert!(styles.contains(&Style::Split));
        assert!(styles.contains(&Style::LeftFocus));
        assert!(styles.contains(&Style::RightFocus));
        assert!(styles.contains(&Style::Intelligent));
        assert!(styles.contains(&Style::IntelligentSplit));
        // "all" should not include Original
        assert!(!styles.contains(&Style::Original));
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
        assert_eq!(styles.len(), 5);
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
}
