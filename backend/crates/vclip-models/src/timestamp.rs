//! Timestamp parsing and validation utilities.
//!
//! This module provides shared timestamp handling for video highlights,
//! supporting formats like HH:MM:SS, HH:MM:SS.mmm, MM:SS, and SS.

/// Maximum reasonable video duration (24 hours in seconds).
pub const MAX_VIDEO_DURATION_SECS: f64 = 86400.0;

/// Parse a timestamp string to total seconds.
///
/// Supports formats:
/// - `HH:MM:SS` or `HH:MM:SS.mmm`
/// - `MM:SS` or `MM:SS.mmm`
/// - `SS` or `SS.mmm`
///
/// # Examples
/// ```
/// use vclip_models::timestamp::parse_timestamp;
/// assert_eq!(parse_timestamp("01:30:00").unwrap(), 5400.0);
/// assert_eq!(parse_timestamp("05:30").unwrap(), 330.0);
/// assert_eq!(parse_timestamp("90").unwrap(), 90.0);
/// ```
pub fn parse_timestamp(ts: &str) -> Result<f64, TimestampError> {
    let ts = ts.trim();
    if ts.is_empty() {
        return Err(TimestampError::Empty);
    }

    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        1 => {
            // Just seconds (SS or SS.mmm)
            let seconds: f64 = parts[0]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("seconds", parts[0].to_string()))?;
            if seconds < 0.0 {
                return Err(TimestampError::Negative);
            }
            Ok(seconds)
        }
        2 => {
            // MM:SS or MM:SS.mmm
            let minutes: f64 = parts[0]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("minutes", parts[0].to_string()))?;
            let seconds: f64 = parts[1]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("seconds", parts[1].to_string()))?;
            if minutes < 0.0 || seconds < 0.0 {
                return Err(TimestampError::Negative);
            }
            Ok(minutes * 60.0 + seconds)
        }
        3 => {
            // HH:MM:SS or HH:MM:SS.mmm
            let hours: f64 = parts[0]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("hours", parts[0].to_string()))?;
            let minutes: f64 = parts[1]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("minutes", parts[1].to_string()))?;
            let seconds: f64 = parts[2]
                .parse()
                .map_err(|_| TimestampError::InvalidValue("seconds", parts[2].to_string()))?;
            if hours < 0.0 || minutes < 0.0 || seconds < 0.0 {
                return Err(TimestampError::Negative);
            }
            Ok(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        _ => Err(TimestampError::InvalidFormat(ts.to_string())),
    }
}

/// Normalize a timestamp to HH:MM:SS or HH:MM:SS.mmm format.
///
/// # Examples
/// ```
/// use vclip_models::timestamp::normalize_timestamp;
/// assert_eq!(normalize_timestamp("5:30").unwrap(), "00:05:30");
/// assert_eq!(normalize_timestamp("90").unwrap(), "00:01:30");
/// ```
pub fn normalize_timestamp(ts: &str) -> Result<String, TimestampError> {
    let total_secs = parse_timestamp(ts)?;
    Ok(format_seconds(total_secs))
}

/// Format seconds into HH:MM:SS or HH:MM:SS.mmm string.
pub fn format_seconds(total_secs: f64) -> String {
    let hours = (total_secs / 3600.0).floor() as u32;
    let mins = ((total_secs % 3600.0) / 60.0).floor() as u32;
    let secs = total_secs % 60.0;

    // Include milliseconds if present
    if (secs - secs.floor()).abs() > 0.0001 {
        format!("{:02}:{:02}:{:06.3}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, secs.floor() as u32)
    }
}

/// Validated timestamp pair with computed duration.
#[derive(Debug, Clone)]
pub struct ValidatedTimestamps {
    /// Normalized start timestamp (HH:MM:SS format)
    pub start: String,
    /// Normalized end timestamp (HH:MM:SS format)
    pub end: String,
    /// Duration in seconds
    pub duration_secs: u32,
    /// Start time in seconds
    pub start_secs: f64,
    /// End time in seconds
    pub end_secs: f64,
}

/// Validate a start/end timestamp pair.
///
/// Checks:
/// - Both timestamps are valid
/// - Start is before end
/// - Neither exceeds max video duration
/// - End doesn't exceed video duration (if provided)
pub fn validate_timestamps(
    start: &str,
    end: &str,
    video_duration: Option<f64>,
) -> Result<ValidatedTimestamps, TimestampError> {
    let start_secs = parse_timestamp(start)?;
    let end_secs = parse_timestamp(end)?;

    // Start must be before end
    if start_secs >= end_secs {
        return Err(TimestampError::StartNotBeforeEnd);
    }

    // Reasonable max check
    if start_secs > MAX_VIDEO_DURATION_SECS || end_secs > MAX_VIDEO_DURATION_SECS {
        return Err(TimestampError::ExceedsMaxDuration(MAX_VIDEO_DURATION_SECS));
    }

    // Video duration check if known
    if let Some(duration) = video_duration {
        if end_secs > duration + 1.0 {
            // Allow 1 second buffer
            return Err(TimestampError::ExceedsVideoDuration {
                end_secs,
                video_duration: duration,
            });
        }
    }

    let normalized_start = format_seconds(start_secs);
    let normalized_end = format_seconds(end_secs);
    let duration_secs = (end_secs - start_secs).max(0.0) as u32;

    Ok(ValidatedTimestamps {
        start: normalized_start,
        end: normalized_end,
        duration_secs,
        start_secs,
        end_secs,
    })
}

/// Timestamp parsing/validation error.
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampError {
    /// Timestamp string is empty
    Empty,
    /// Timestamp contains negative values
    Negative,
    /// Invalid numeric value for a component
    InvalidValue(&'static str, String),
    /// Invalid timestamp format
    InvalidFormat(String),
    /// Start time is not before end time
    StartNotBeforeEnd,
    /// Timestamp exceeds maximum allowed duration
    ExceedsMaxDuration(f64),
    /// End time exceeds video duration
    ExceedsVideoDuration { end_secs: f64, video_duration: f64 },
}

impl std::fmt::Display for TimestampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Timestamp cannot be empty"),
            Self::Negative => write!(f, "Timestamp cannot be negative"),
            Self::InvalidValue(component, value) => {
                write!(f, "Invalid {} value: {}", component, value)
            }
            Self::InvalidFormat(ts) => write!(
                f,
                "Invalid timestamp format '{}'. Use HH:MM:SS, HH:MM:SS.mmm, MM:SS, or MM:SS.mmm",
                ts
            ),
            Self::StartNotBeforeEnd => write!(f, "Start time must be before end time"),
            Self::ExceedsMaxDuration(max) => {
                write!(f, "Timestamps exceed maximum allowed duration ({} hours)", max / 3600.0)
            }
            Self::ExceedsVideoDuration { end_secs, video_duration } => write!(
                f,
                "End time ({:.1}s) exceeds video duration ({:.1}s)",
                end_secs, video_duration
            ),
        }
    }
}

impl std::error::Error for TimestampError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_hh_mm_ss() {
        assert_eq!(parse_timestamp("00:00:00").unwrap(), 0.0);
        assert_eq!(parse_timestamp("00:01:00").unwrap(), 60.0);
        assert_eq!(parse_timestamp("01:00:00").unwrap(), 3600.0);
        assert_eq!(parse_timestamp("01:30:45").unwrap(), 5445.0);
    }

    #[test]
    fn test_parse_timestamp_mm_ss() {
        assert_eq!(parse_timestamp("05:30").unwrap(), 330.0);
        assert_eq!(parse_timestamp("53:53").unwrap(), 3233.0);
    }

    #[test]
    fn test_parse_timestamp_ss() {
        assert_eq!(parse_timestamp("90").unwrap(), 90.0);
        assert_eq!(parse_timestamp("0").unwrap(), 0.0);
    }

    #[test]
    fn test_parse_timestamp_with_milliseconds() {
        let result = parse_timestamp("00:00:30.500").unwrap();
        assert!((result - 30.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_timestamp_errors() {
        assert!(matches!(parse_timestamp(""), Err(TimestampError::Empty)));
        assert!(matches!(parse_timestamp("  "), Err(TimestampError::Empty)));
        assert!(matches!(parse_timestamp("abc"), Err(TimestampError::InvalidValue(_, _))));
        assert!(matches!(parse_timestamp("1:2:3:4"), Err(TimestampError::InvalidFormat(_))));
    }

    #[test]
    fn test_normalize_timestamp() {
        assert_eq!(normalize_timestamp("5:30").unwrap(), "00:05:30");
        assert_eq!(normalize_timestamp("90").unwrap(), "00:01:30");
        assert_eq!(normalize_timestamp("1:30:00").unwrap(), "01:30:00");
    }

    #[test]
    fn test_validate_timestamps_valid() {
        let result = validate_timestamps("00:00:00", "00:01:30", None).unwrap();
        assert_eq!(result.start, "00:00:00");
        assert_eq!(result.end, "00:01:30");
        assert_eq!(result.duration_secs, 90);
    }

    #[test]
    fn test_validate_timestamps_start_after_end() {
        let result = validate_timestamps("00:02:00", "00:01:00", None);
        assert!(matches!(result, Err(TimestampError::StartNotBeforeEnd)));
    }

    #[test]
    fn test_validate_timestamps_exceeds_video_duration() {
        let result = validate_timestamps("00:00:00", "00:05:00", Some(240.0));
        assert!(matches!(result, Err(TimestampError::ExceedsVideoDuration { .. })));
    }

    #[test]
    fn test_format_seconds() {
        assert_eq!(format_seconds(0.0), "00:00:00");
        assert_eq!(format_seconds(90.0), "00:01:30");
        assert_eq!(format_seconds(3661.0), "01:01:01");
    }
}
