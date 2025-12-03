//! Highlight (scene) models.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Hook category for a highlight.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HighlightCategory {
    Emotional,
    Educational,
    Controversial,
    Inspirational,
    Humorous,
    Dramatic,
    Surprising,
    #[serde(other)]
    Other,
}

/// A highlight/scene detected in the video.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Highlight {
    /// Unique ID within the video (1-indexed)
    pub id: u32,

    /// Scene title
    pub title: String,

    /// Start timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub start: String,

    /// End timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub end: String,

    /// Duration in seconds
    pub duration: u32,

    /// Hook category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_category: Option<HighlightCategory>,

    /// Reason why this is a good clip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Description of the scene
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Highlight {
    /// Create a new highlight.
    pub fn new(id: u32, title: impl Into<String>, start: impl Into<String>, end: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            start: start.into(),
            end: end.into(),
            duration: 0, // Will be calculated
            hook_category: None,
            reason: None,
            description: None,
        }
    }

    /// Calculate duration from start/end timestamps.
    pub fn with_calculated_duration(mut self) -> Self {
        if let (Ok(start_secs), Ok(end_secs)) = (
            parse_timestamp(&self.start),
            parse_timestamp(&self.end),
        ) {
            self.duration = (end_secs - start_secs).max(0.0) as u32;
        }
        self
    }
}

/// Highlights data stored in R2 (highlights.json).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HighlightsData {
    /// List of highlights
    pub highlights: Vec<Highlight>,

    /// Video URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,

    /// Video title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,

    /// Custom prompt used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
}

impl HighlightsData {
    pub fn new(highlights: Vec<Highlight>) -> Self {
        Self {
            highlights,
            video_url: None,
            video_title: None,
            custom_prompt: None,
        }
    }
}

/// Parse a timestamp string (HH:MM:SS or HH:MM:SS.mmm) to total seconds.
fn parse_timestamp(ts: &str) -> Result<f64, ()> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return Err(());
    }

    let hours: f64 = parts[0].parse().map_err(|_| ())?;
    let minutes: f64 = parts[1].parse().map_err(|_| ())?;
    let seconds: f64 = parts[2].parse().map_err(|_| ())?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp() {
        assert_eq!(parse_timestamp("00:00:00").unwrap(), 0.0);
        assert_eq!(parse_timestamp("00:01:00").unwrap(), 60.0);
        assert_eq!(parse_timestamp("01:00:00").unwrap(), 3600.0);
        assert!((parse_timestamp("00:00:30.500").unwrap() - 30.5).abs() < 0.001);
    }

    #[test]
    fn test_highlight_duration() {
        let h = Highlight::new(1, "Test", "00:00:00", "00:01:30").with_calculated_duration();
        assert_eq!(h.duration, 90);
    }
}
