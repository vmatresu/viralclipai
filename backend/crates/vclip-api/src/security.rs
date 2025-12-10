//! Security utilities for input validation and sanitization.
//!
//! This module provides:
//! - URL validation with whitelist support (SSRF protection)
//! - Input sanitization utilities
//! - Rate limiter cache with TTL cleanup

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use tracing::warn;
use url::Url;

/// Maximum URL length to prevent DoS attacks.
const MAX_URL_LENGTH: usize = 2048;

/// Maximum prompt length.
pub const MAX_PROMPT_LENGTH: usize = 5000;

/// Maximum title length.
pub const MAX_TITLE_LENGTH: usize = 500;

/// Allowed video URL domains (whitelist for SSRF protection).
static ALLOWED_DOMAINS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        // YouTube
        "youtube.com",
        "www.youtube.com",
        "youtu.be",
        "m.youtube.com",
        // Vimeo
        "vimeo.com",
        "www.vimeo.com",
        "player.vimeo.com",
        // Loom
        "loom.com",
        "www.loom.com",
        // Wistia
        "wistia.com",
        "www.wistia.com",
        "fast.wistia.com",
        // Dailymotion
        "dailymotion.com",
        "www.dailymotion.com",
        // TikTok
        "tiktok.com",
        "www.tiktok.com",
        "vm.tiktok.com",
        // Twitter/X
        "twitter.com",
        "www.twitter.com",
        "x.com",
        "www.x.com",
        // Instagram
        "instagram.com",
        "www.instagram.com",
        // Facebook
        "facebook.com",
        "www.facebook.com",
        "fb.watch",
        // Twitch
        "twitch.tv",
        "www.twitch.tv",
        "clips.twitch.tv",
        // Streamable
        "streamable.com",
        "www.streamable.com",
        // Direct video file hosting
        "cdn.viralclipai.io",
    ])
});

/// Blocked URL patterns (sensitive endpoints).
static BLOCKED_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // Block internal IP ranges
        Regex::new(r"^https?://127\.").unwrap(),
        Regex::new(r"^https?://localhost").unwrap(),
        Regex::new(r"^https?://10\.").unwrap(),
        Regex::new(r"^https?://172\.(1[6-9]|2[0-9]|3[0-1])\.").unwrap(),
        Regex::new(r"^https?://192\.168\.").unwrap(),
        Regex::new(r"^https?://169\.254\.").unwrap(),
        Regex::new(r"^https?://\[::1\]").unwrap(),
        Regex::new(r"^https?://\[fd").unwrap(),
        Regex::new(r"^https?://\[fe80").unwrap(),
        // Block cloud metadata endpoints
        Regex::new(r"^https?://metadata\.").unwrap(),
        Regex::new(r"^https?://169\.254\.169\.254").unwrap(),
        Regex::new(r"^https?://metadata\.google\.internal").unwrap(),
    ]
});

/// Result of URL validation.
#[derive(Debug)]
pub enum UrlValidationResult {
    /// URL is valid and allowed.
    Valid(String),
    /// URL is malformed or uses an unsupported protocol.
    Invalid(String),
    /// URL domain is not in the whitelist.
    DomainNotAllowed(String),
    /// URL matches a blocked pattern (e.g., internal IPs).
    Blocked(String),
    /// URL exceeds maximum length.
    TooLong,
}

impl UrlValidationResult {
    /// Convert to Result for easy error handling.
    pub fn into_result(self) -> Result<String, String> {
        match self {
            Self::Valid(url) => Ok(url),
            Self::Invalid(msg) => Err(msg),
            Self::DomainNotAllowed(domain) => {
                Err(format!("Domain '{}' is not allowed. Please use a supported video platform (YouTube, Vimeo, TikTok, etc.)", domain))
            }
            Self::Blocked(reason) => Err(reason),
            Self::TooLong => Err(format!("URL exceeds maximum length of {} characters", MAX_URL_LENGTH)),
        }
    }
}

/// Validate a video URL for security and domain whitelist.
///
/// This function performs:
/// - Length validation
/// - Protocol validation (only http/https)
/// - Domain whitelist check (SSRF protection)
/// - Blocked pattern check (internal IPs, metadata endpoints)
pub fn validate_video_url(url: &str) -> UrlValidationResult {
    // Check length
    if url.len() > MAX_URL_LENGTH {
        return UrlValidationResult::TooLong;
    }

    // Trim and normalize
    let url = url.trim();
    if url.is_empty() {
        return UrlValidationResult::Invalid("URL cannot be empty".to_string());
    }

    // Parse URL
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(e) => return UrlValidationResult::Invalid(format!("Invalid URL format: {}", e)),
    };

    // Check protocol
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return UrlValidationResult::Invalid(format!(
                "Invalid protocol '{}'. Only HTTP and HTTPS are allowed.",
                scheme
            ))
        }
    }

    // Check for blocked patterns (internal IPs, metadata endpoints)
    for pattern in BLOCKED_PATTERNS.iter() {
        if pattern.is_match(url) {
            warn!(url = %url, "Blocked URL pattern detected");
            return UrlValidationResult::Blocked(
                "URL appears to target an internal or restricted endpoint".to_string(),
            );
        }
    }

    // Extract domain
    let domain = match parsed.host_str() {
        Some(d) => d.to_lowercase(),
        None => return UrlValidationResult::Invalid("URL must have a valid domain".to_string()),
    };

    // Check domain whitelist
    if !is_domain_allowed(&domain) {
        return UrlValidationResult::DomainNotAllowed(domain);
    }

    // URL is valid
    UrlValidationResult::Valid(url.to_string())
}

/// Check if a domain or any of its parent domains are in the whitelist.
fn is_domain_allowed(domain: &str) -> bool {
    // Direct match
    if ALLOWED_DOMAINS.contains(domain) {
        return true;
    }

    // Check parent domains (e.g., allow "video.youtube.com" because "youtube.com" is allowed)
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() >= 2 {
        // Try with just the last two parts (domain + TLD)
        let parent = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        if ALLOWED_DOMAINS.contains(parent.as_str()) {
            return true;
        }
    }

    false
}

/// Sanitize a user-provided string for safe logging and storage.
///
/// This removes or escapes potentially dangerous characters.
pub fn sanitize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(MAX_PROMPT_LENGTH)
        .collect()
}

/// Sanitize a title for safe storage.
pub fn sanitize_title(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() > MAX_TITLE_LENGTH {
        trimmed.chars().take(MAX_TITLE_LENGTH).collect()
    } else {
        trimmed.to_string()
    }
}

/// Validate video ID format.
///
/// Valid format: alphanumeric characters and hyphens only, 8-64 chars.
pub fn is_valid_video_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.len() < 8 {
        return false;
    }
    id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Validate clip name format.
///
/// Valid format: alphanumeric, hyphens, underscores, dots. No path traversal.
pub fn is_valid_clip_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 256 {
        return false;
    }
    // Block path traversal
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return false;
    }
    // Allow alphanumeric, hyphen, underscore, dot
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_youtube_urls() {
        assert!(matches!(
            validate_video_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            UrlValidationResult::Valid(_)
        ));
        assert!(matches!(
            validate_video_url("https://youtu.be/dQw4w9WgXcQ"),
            UrlValidationResult::Valid(_)
        ));
    }

    #[test]
    fn test_valid_vimeo_urls() {
        assert!(matches!(
            validate_video_url("https://vimeo.com/123456789"),
            UrlValidationResult::Valid(_)
        ));
    }

    #[test]
    fn test_blocked_internal_ips() {
        assert!(matches!(
            validate_video_url("http://127.0.0.1/video.mp4"),
            UrlValidationResult::Blocked(_)
        ));
        assert!(matches!(
            validate_video_url("http://localhost/video.mp4"),
            UrlValidationResult::Blocked(_)
        ));
        assert!(matches!(
            validate_video_url("http://192.168.1.1/video.mp4"),
            UrlValidationResult::Blocked(_)
        ));
        assert!(matches!(
            validate_video_url("http://169.254.169.254/latest/meta-data/"),
            UrlValidationResult::Blocked(_)
        ));
    }

    #[test]
    fn test_invalid_domains() {
        assert!(matches!(
            validate_video_url("https://malicious-site.com/video.mp4"),
            UrlValidationResult::DomainNotAllowed(_)
        ));
    }

    #[test]
    fn test_invalid_protocols() {
        assert!(matches!(
            validate_video_url("ftp://youtube.com/video"),
            UrlValidationResult::Invalid(_)
        ));
        assert!(matches!(
            validate_video_url("javascript:alert(1)"),
            UrlValidationResult::Invalid(_)
        ));
    }

    #[test]
    fn test_video_id_validation() {
        assert!(is_valid_video_id("12345678"));
        assert!(is_valid_video_id("abc-def-123-456"));
        assert!(!is_valid_video_id("short"));
        assert!(!is_valid_video_id("has/slash"));
        assert!(!is_valid_video_id("has..dots"));
    }

    #[test]
    fn test_clip_name_validation() {
        assert!(is_valid_clip_name("video.mp4"));
        assert!(is_valid_clip_name("clip_001-final.mp4"));
        assert!(!is_valid_clip_name("../etc/passwd"));
        assert!(!is_valid_clip_name("path/to/file.mp4"));
    }
}
