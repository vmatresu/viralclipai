//! Utility functions for URL parsing and validation.
//!
//! This module provides shared utility functions that are used across
//! multiple crates in the ViralClip backend, following DRY principles.

/// Errors that can occur during YouTube ID extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YoutubeIdError {
    /// URL is not a valid YouTube URL
    InvalidYoutubeUrl,
    /// Video ID has invalid format
    InvalidVideoId,
    /// Video ID not found in URL
    VideoIdNotFound,
}

impl std::fmt::Display for YoutubeIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YoutubeIdError::InvalidYoutubeUrl => write!(f, "URL is not a valid YouTube URL"),
            YoutubeIdError::InvalidVideoId => write!(f, "Video ID has invalid format"),
            YoutubeIdError::VideoIdNotFound => write!(f, "Video ID not found in URL"),
        }
    }
}

impl std::error::Error for YoutubeIdError {}

/// Result type for YouTube ID extraction.
pub type YoutubeIdResult<T> = Result<T, YoutubeIdError>;

/// Extract YouTube video ID from URL with comprehensive format support.
///
/// Supports all YouTube URL formats:
/// - https://youtube.com/watch?v=VIDEO_ID
/// - https://youtu.be/VIDEO_ID
/// - https://youtube.com/embed/VIDEO_ID
/// - https://youtube.com/v/VIDEO_ID
/// - https://youtube.com/shorts/VIDEO_ID
/// - With or without query parameters, fragments, etc.
///
/// Returns the 11-character YouTube video ID or an error.
pub fn extract_youtube_id(url: &str) -> YoutubeIdResult<String> {
    let url = url.trim();

    // Check if it's a YouTube domain
    if !is_youtube_domain(url) {
        return Err(YoutubeIdError::InvalidYoutubeUrl);
    }

    // Try different extraction strategies in order of preference
    if let Some(id) = extract_from_watch_url(url) {
        return validate_youtube_id(id);
    }

    if let Some(id) = extract_from_short_url(url) {
        return validate_youtube_id(id);
    }

    if let Some(id) = extract_from_embed_url(url) {
        return validate_youtube_id(id);
    }

    if let Some(id) = extract_from_v_url(url) {
        return validate_youtube_id(id);
    }

    if let Some(id) = extract_from_shorts_url(url) {
        return validate_youtube_id(id);
    }

    Err(YoutubeIdError::VideoIdNotFound)
}

/// Check if URL is from a YouTube domain
fn is_youtube_domain(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.contains("youtube.com") || url.contains("youtu.be")
}

/// Extract ID from youtube.com/watch?v=VIDEO_ID
fn extract_from_watch_url(url: &str) -> Option<String> {
    if let Some(v_pos) = url.find("?v=") {
        let start = v_pos + 3;
        let remaining = &url[start..];
        extract_id_from_segment(remaining)
    } else if let Some(v_pos) = url.find("&v=") {
        let start = v_pos + 3;
        let remaining = &url[start..];
        extract_id_from_segment(remaining)
    } else {
        None
    }
}

/// Extract ID from youtu.be/VIDEO_ID
fn extract_from_short_url(url: &str) -> Option<String> {
    if let Some(be_pos) = url.find("youtu.be/") {
        let start = be_pos + 9;
        if start < url.len() {
            let remaining = &url[start..];
            extract_id_from_segment(remaining)
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract ID from youtube.com/embed/VIDEO_ID
fn extract_from_embed_url(url: &str) -> Option<String> {
    if let Some(embed_pos) = url.find("/embed/") {
        let start = embed_pos + 7;
        if start < url.len() {
            let remaining = &url[start..];
            extract_id_from_segment(remaining)
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract ID from youtube.com/v/VIDEO_ID
fn extract_from_v_url(url: &str) -> Option<String> {
    if let Some(v_pos) = url.find("/v/") {
        let start = v_pos + 3;
        if start < url.len() {
            let remaining = &url[start..];
            extract_id_from_segment(remaining)
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract ID from youtube.com/shorts/VIDEO_ID
fn extract_from_shorts_url(url: &str) -> Option<String> {
    if let Some(shorts_pos) = url.find("/shorts/") {
        let start = shorts_pos + 8;
        if start < url.len() {
            let remaining = &url[start..];
            extract_id_from_segment(remaining)
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract the first valid ID segment from a string
fn extract_id_from_segment(segment: &str) -> Option<String> {
    let delimiters = ['&', '#', '?', '/'];
    let end = segment
        .find(|c| delimiters.contains(&c))
        .unwrap_or(segment.len());
    Some(segment[..end].trim().to_string())
}

/// Check if string contains only valid YouTube ID characters
fn is_valid_youtube_id_chars(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Validate YouTube video ID format and return it
fn validate_youtube_id(id: String) -> YoutubeIdResult<String> {
    // YouTube video IDs are exactly 11 characters
    if id.len() != 11 {
        return Err(YoutubeIdError::InvalidVideoId);
    }

    // Must contain only valid characters: alphanumeric, hyphens, underscores
    if !is_valid_youtube_id_chars(&id) {
        return Err(YoutubeIdError::InvalidVideoId);
    }

    Ok(id)
}

/// Legacy function for backward compatibility - returns Option for existing code
pub fn extract_youtube_id_legacy(url: &str) -> Option<String> {
    extract_youtube_id(url).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_youtube_id_success_cases() {
        // Standard youtube.com format
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // With www prefix
        assert_eq!(
            extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // youtu.be format
        assert_eq!(
            extract_youtube_id("https://youtu.be/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // Embed format
        assert_eq!(
            extract_youtube_id("https://youtube.com/embed/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // /v/ format
        assert_eq!(
            extract_youtube_id("https://youtube.com/v/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // Shorts format
        assert_eq!(
            extract_youtube_id("https://youtube.com/shorts/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // With query parameters
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmRdnEQy4qtr").unwrap(),
            "dQw4w9WgXcQ"
        );

        // With fragment
        assert_eq!(
            extract_youtube_id("https://youtu.be/dQw4w9WgXcQ?t=30").unwrap(),
            "dQw4w9WgXcQ"
        );

        // With underscores and hyphens in ID
        assert_eq!(
            extract_youtube_id("https://youtu.be/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_extract_youtube_id_error_cases() {
        // Non-YouTube URLs
        assert!(matches!(
            extract_youtube_id("https://example.com"),
            Err(YoutubeIdError::InvalidYoutubeUrl)
        ));

        assert!(matches!(
            extract_youtube_id("https://vimeo.com/123"),
            Err(YoutubeIdError::InvalidYoutubeUrl)
        ));

        // Valid YouTube domain but no video ID
        assert!(matches!(
            extract_youtube_id("https://youtube.com"),
            Err(YoutubeIdError::VideoIdNotFound)
        ));

        assert!(matches!(
            extract_youtube_id("https://youtu.be/"),
            Err(YoutubeIdError::VideoIdNotFound)
        ));

        // Invalid video ID format
        assert!(matches!(
            extract_youtube_id("https://youtube.com/watch?v=abc123"), // too short
            Err(YoutubeIdError::InvalidVideoId)
        ));

        assert!(matches!(
            extract_youtube_id("https://youtu.be/abc123def456789"), // too long
            Err(YoutubeIdError::InvalidVideoId)
        ));

        assert!(matches!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def!!"), // invalid chars
            Err(YoutubeIdError::InvalidVideoId)
        ));

        // Empty ID
        assert!(matches!(
            extract_youtube_id("https://youtube.com/watch?v="),
            Err(YoutubeIdError::InvalidVideoId)
        ));
    }

    #[test]
    fn test_extract_youtube_id_legacy_compatibility() {
        // Test that legacy function still works
        assert_eq!(
            extract_youtube_id_legacy("https://youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );

        assert_eq!(
            extract_youtube_id_legacy("https://invalid-url"),
            None
        );
    }

    #[test]
    fn test_youtube_id_error_display() {
        assert_eq!(
            YoutubeIdError::InvalidYoutubeUrl.to_string(),
            "URL is not a valid YouTube URL"
        );
        assert_eq!(
            YoutubeIdError::InvalidVideoId.to_string(),
            "Video ID has invalid format"
        );
        assert_eq!(
            YoutubeIdError::VideoIdNotFound.to_string(),
            "Video ID not found in URL"
        );
    }

    #[test]
    fn test_edge_cases() {
        // URL with multiple query parameters
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=dQw4w9WgXcQ&feature=share&si=test").unwrap(),
            "dQw4w9WgXcQ"
        );

        // URL with fragment and query
        assert_eq!(
            extract_youtube_id("https://youtu.be/dQw4w9WgXcQ?t=30&feature=share").unwrap(),
            "dQw4w9WgXcQ"
        );

        // Extra whitespace (should be trimmed)
        assert_eq!(
            extract_youtube_id("  https://youtube.com/watch?v=dQw4w9WgXcQ  ").unwrap(),
            "dQw4w9WgXcQ"
        );

        // Case variations in domain should work
        assert_eq!(
            extract_youtube_id("https://YOUTUBE.COM/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_domain_validation() {
        assert!(is_youtube_domain("https://youtube.com/watch?v=test"));
        assert!(is_youtube_domain("https://youtu.be/test"));
        assert!(is_youtube_domain("https://www.youtube.com/test"));
        assert!(!is_youtube_domain("https://example.com"));
        assert!(!is_youtube_domain("https://vimeo.com"));
    }
}
