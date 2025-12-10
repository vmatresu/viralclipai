//! YouTube URL parsing, validation, and yt-dlp configuration generation.
//!
//! This module provides comprehensive YouTube URL analysis and generates
//! safe yt-dlp configuration plans for transcript and video download.
//!
//! # Security
//! - URLs are treated as untrusted input
//! - Only YouTube domains are accepted
//! - Video IDs are strictly validated (11 chars, alphanumeric + `-_`)
//! - No shell command execution or external API calls

use serde::{Deserialize, Serialize};

use crate::utils::{extract_host, extract_youtube_id, is_youtube_domain, YoutubeIdError};

// ============================================================================
// Input Types
// ============================================================================

/// Input configuration for YouTube URL analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeUrlInput {
    /// Raw user-pasted URL (untrusted)
    pub raw_url: String,

    /// Ordered list of preferred subtitle languages (e.g., ["en"])
    #[serde(default = "default_sub_langs")]
    pub preferred_sub_langs: Vec<String>,

    /// Allow auto-generated subtitles as fallback
    #[serde(default = "default_true")]
    pub allow_auto_subs: bool,

    /// Live capture mode: "from_start" or "from_now"
    #[serde(default = "default_live_capture_mode")]
    pub live_capture_mode: LiveCaptureMode,

    /// Upper-bound hint for reasonable content length (seconds). Currently
    /// informational for downstream planners; this module does not change flags
    /// based on the value.
    #[serde(default = "default_max_duration")]
    pub max_expected_duration_sec: u64,
}

fn default_sub_langs() -> Vec<String> {
    vec!["en".to_string()]
}

fn default_true() -> bool {
    true
}

fn default_live_capture_mode() -> LiveCaptureMode {
    LiveCaptureMode::FromStart
}

fn default_max_duration() -> u64 {
    21600 // 6 hours
}

// ============================================================================
// Output Types
// ============================================================================

/// Complete YouTube URL analysis and yt-dlp configuration result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeUrlConfig {
    /// Normalized canonical watch URL
    pub normalized_url: Option<String>,

    /// Extracted 11-character video ID
    pub video_id: Option<String>,

    /// Classification of the URL type
    pub url_type: UrlType,

    /// Whether the URL explicitly indicates a Short
    pub is_shorts: bool,

    /// Whether the URL refers to an active livestream (conservative; never true
    /// without external metadata)
    pub is_live: bool,

    /// Whether subtitles are expected to be available (unknown without API call)
    pub has_subtitles: Option<bool>,

    /// Configuration for subtitle/transcript download
    pub subtitle_plan: SubtitlePlan,

    /// Configuration for video file download
    pub video_download_plan: VideoDownloadPlan,

    /// Live stream handling configuration
    pub live_handling: LiveHandling,

    /// Validation results and any errors
    pub validation: ValidationResult,
}

/// Classification of YouTube URL types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlType {
    /// Standard watch-style video
    Video,
    /// YouTube Shorts (vertical short-form)
    Shorts,
    /// Channel/user URL pointing to a single video
    ChannelItem,
    /// Playlist URL with a specific video selected
    PlaylistItem,
    /// Active livestream (reserved for future metadata-aware classification)
    Live,
    /// Finished livestream VOD (reserved for future metadata-aware classification)
    LiveVod,
    /// URL that cannot be processed
    Unsupported,
}

impl std::fmt::Display for UrlType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UrlType::Video => write!(f, "video"),
            UrlType::Shorts => write!(f, "shorts"),
            UrlType::ChannelItem => write!(f, "channel_item"),
            UrlType::PlaylistItem => write!(f, "playlist_item"),
            UrlType::Live => write!(f, "live"),
            UrlType::LiveVod => write!(f, "live_vod"),
            UrlType::Unsupported => write!(f, "unsupported"),
        }
    }
}

/// Live capture mode for streaming content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveCaptureMode {
    /// Capture from the beginning of the stream
    FromStart,
    /// Capture from the current moment onward
    FromNow,
}

impl Default for LiveCaptureMode {
    fn default() -> Self {
        Self::FromStart
    }
}

/// Configuration for subtitle/transcript download via yt-dlp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitlePlan {
    /// Whether subtitle download is enabled
    pub enabled: bool,

    /// Preferred subtitle languages in order
    pub preferred_languages: Vec<String>,

    /// Allow auto-generated subtitles
    pub allow_auto_subs: bool,

    /// Exclude live chat transcripts
    pub exclude_live_chat: bool,

    /// yt-dlp command-line flags for subtitle download
    pub yt_dlp_flags: Vec<String>,
}

/// Configuration for video file download via yt-dlp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDownloadPlan {
    /// Whether video download is enabled
    pub enabled: bool,

    /// Preferred container format
    pub preferred_container: String,

    /// Preferred codecs in order [video, audio]
    pub preferred_codecs: Vec<String>,

    /// Whether this requires chunked live capture
    pub is_chunked_live_capture: bool,

    /// yt-dlp command-line flags for video download
    pub yt_dlp_flags: Vec<String>,
}

/// Live stream handling configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveHandling {
    /// Capture mode for live content
    pub live_capture_mode: LiveCaptureMode,

    /// Optional download sections (time ranges)
    /// Currently always None as input doesn't provide time ranges
    pub download_sections: Option<String>,
}

/// Validation results for the URL analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether this is a valid, supported YouTube URL
    pub is_supported_youtube_url: bool,

    /// List of validation errors or warnings
    pub errors: Vec<String>,
}

// ============================================================================
// URL Analysis Implementation
// ============================================================================

/// Analyzes a YouTube URL and generates yt-dlp configuration.
///
/// This function performs comprehensive URL validation, normalization,
/// and classification, then generates safe yt-dlp configuration plans.
///
/// # Security
/// - URLs are treated as untrusted input
/// - Only YouTube domains are accepted
/// - Video IDs are strictly validated
/// - No external calls or command execution
///
/// # Example
/// ```
/// use vclip_models::youtube_url_config::{analyze_youtube_url, YoutubeUrlInput};
///
/// let input = YoutubeUrlInput {
///     raw_url: "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
///     preferred_sub_langs: vec!["en".to_string()],
///     allow_auto_subs: true,
///     live_capture_mode: Default::default(),
///     max_expected_duration_sec: 21600,
/// };
///
/// let config = analyze_youtube_url(&input);
/// assert!(config.validation.is_supported_youtube_url);
/// assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
/// ```
pub fn analyze_youtube_url(input: &YoutubeUrlInput) -> YoutubeUrlConfig {
    let trimmed_url = input.raw_url.trim();
    let mut errors: Vec<String> = Vec::new();

    // Step 1: Domain validation
    if !is_youtube_domain(trimmed_url) {
        let domain = extract_host(trimmed_url).unwrap_or_else(|| "unknown".to_string());
        errors.push(format!("Non-YouTube domain: {}", domain));

        return YoutubeUrlConfig {
            normalized_url: None,
            video_id: None,
            url_type: UrlType::Unsupported,
            is_shorts: false,
            is_live: false,
            has_subtitles: None,
            subtitle_plan: build_disabled_subtitle_plan(),
            video_download_plan: build_disabled_video_plan(),
            live_handling: LiveHandling {
                live_capture_mode: input.live_capture_mode,
                download_sections: None,
            },
            validation: ValidationResult {
                is_supported_youtube_url: false,
                errors,
            },
        };
    }

    // Step 2: Video ID extraction
    let video_id = match extract_youtube_id(trimmed_url) {
        Ok(id) => id,
        Err(e) => {
            let error_msg = match e {
                YoutubeIdError::InvalidYoutubeUrl => "URL is not a valid YouTube URL".to_string(),
                YoutubeIdError::InvalidVideoId => {
                    "Video ID has invalid format (must be 11 alphanumeric characters)".to_string()
                }
                YoutubeIdError::VideoIdNotFound => {
                    // Check for specific unsupported patterns
                    if is_playlist_only_url(trimmed_url) {
                        "Playlist URL without specific video selected (v= parameter missing)"
                            .to_string()
                    } else if is_channel_url(trimmed_url) {
                        "Channel/user URL without specific video".to_string()
                    } else {
                        "Could not extract a valid 11-character YouTube video ID".to_string()
                    }
                }
            };
            errors.push(error_msg);

            return YoutubeUrlConfig {
                normalized_url: None,
                video_id: None,
                url_type: UrlType::Unsupported,
                is_shorts: false,
                is_live: false,
                has_subtitles: None,
                subtitle_plan: build_disabled_subtitle_plan(),
                video_download_plan: build_disabled_video_plan(),
                live_handling: LiveHandling {
                    live_capture_mode: input.live_capture_mode,
                    download_sections: None,
                },
                validation: ValidationResult {
                    is_supported_youtube_url: false,
                    errors,
                },
            };
        }
    };

    // Step 3: URL type classification
    let url_classification = classify_url(trimmed_url);

    // Step 4: Sanitize preferred subtitle languages (warnings are non-fatal)
    let (sanitized_langs, lang_warnings) = sanitize_preferred_sub_langs(&input.preferred_sub_langs);
    errors.extend(lang_warnings);

    // Step 5: Build normalized URL
    let normalized_url = format!("https://www.youtube.com/watch?v={}", video_id);

    // Step 6: Build configuration plans
    let subtitle_plan = build_subtitle_plan(input, &sanitized_langs);
    let video_download_plan = build_video_download_plan(&url_classification, input);

    YoutubeUrlConfig {
        normalized_url: Some(normalized_url),
        video_id: Some(video_id),
        url_type: url_classification.url_type,
        is_shorts: url_classification.is_shorts,
        is_live: url_classification.is_live,
        has_subtitles: None, // Cannot determine without API call
        subtitle_plan,
        video_download_plan,
        live_handling: LiveHandling {
            live_capture_mode: input.live_capture_mode,
            download_sections: None, // No time ranges in current input schema
        },
        validation: ValidationResult {
            is_supported_youtube_url: true,
            errors,
        },
    }
}

// ============================================================================
// URL Classification
// ============================================================================

/// Internal classification result.
struct UrlClassification {
    url_type: UrlType,
    is_shorts: bool,
    is_live: bool,
}

/// Classify the URL type based on path and query parameters.
fn classify_url(url: &str) -> UrlClassification {
    let url_lower = url.to_lowercase();

    // Check for Shorts
    if url_lower.contains("/shorts/") {
        return UrlClassification {
            url_type: UrlType::Shorts,
            is_shorts: true,
            is_live: false,
        };
    }

    // Check for playlist with video
    if has_playlist_param(url) && has_video_param(url) {
        return UrlClassification {
            url_type: UrlType::PlaylistItem,
            is_shorts: false,
            is_live: false,
        };
    }

    // Check for channel context
    // Note: we can only detect channel_item if the URL structure suggests it
    // AND we successfully extracted a video ID
    if is_channel_url(url) && has_video_param(url) {
        return UrlClassification {
            url_type: UrlType::ChannelItem,
            is_shorts: false,
            is_live: false,
        };
    }

    // Check for explicit live indicators in URL
    // Note: Without API access, we CANNOT reliably determine if a video
    // is currently live. The URL patterns below are heuristics only.
    if url_lower.contains("/live") && !url_lower.contains("/live/") {
        // /live endpoint often indicates live content, but could be VOD
        // Conservative: treat as video, not live
        return UrlClassification {
            url_type: UrlType::Video,
            is_shorts: false,
            is_live: false,
        };
    }

    // Default: standard video
    // Conservative default: Live/LiveVod are reserved for future metadata-aware
    // classification. We intentionally keep is_live = false here.
    UrlClassification {
        url_type: UrlType::Video,
        is_shorts: false,
        is_live: false,
    }
}

// ============================================================================
// URL Parsing Helpers
// ============================================================================

/// Check if URL contains a playlist parameter without video selection.
fn is_playlist_only_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    (url_lower.contains("list=") || url_lower.contains("/playlist"))
        && !url_lower.contains("v=")
        && !url_lower.contains("/watch")
}

/// Check if URL has a playlist parameter.
fn has_playlist_param(url: &str) -> bool {
    url.to_lowercase().contains("list=")
}

/// Check if URL has a video parameter.
fn has_video_param(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.contains("v=") || url_lower.contains("/watch")
}

/// Check if URL is a channel/user URL.
fn is_channel_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.contains("/channel/")
        || url_lower.contains("/user/")
        || url_lower.contains("/c/")
        || url_lower.contains("/@")
}

// ============================================================================
// Input Sanitization
// ============================================================================

/// Sanitize and validate preferred subtitle language codes.
///
/// - Allows ASCII alphanumeric characters plus `-` and `_`
/// - Trims whitespace and lowercases values
/// - Rejects entries longer than 16 characters
/// - Returns sanitized list and non-fatal warnings
fn sanitize_preferred_sub_langs(langs: &[String]) -> (Vec<String>, Vec<String>) {
    let mut sanitized = Vec::new();
    let mut warnings = Vec::new();

    for lang in langs {
        let trimmed = lang.trim();

        if trimmed.is_empty() {
            warnings.push("Ignored empty subtitle language entry".to_string());
            continue;
        }

        if trimmed.len() > 16 {
            warnings.push(format!(
                "Ignored subtitle language '{}' (too long, max 16 characters)",
                trimmed
            ));
            continue;
        }

        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            warnings.push(format!(
                "Ignored subtitle language '{}' (only ASCII alphanumeric, '-' and '_' allowed)",
                trimmed
            ));
            continue;
        }

        sanitized.push(trimmed.to_ascii_lowercase());
    }

    if sanitized.is_empty() && !langs.is_empty() {
        warnings.push(
            "All preferred subtitle languages were invalid; defaulting to 'en' for yt-dlp flags"
                .to_string(),
        );
    }

    (sanitized, warnings)
}

// ============================================================================
// Plan Builders
// ============================================================================

/// Build subtitle download plan based on input configuration.
fn build_subtitle_plan(input: &YoutubeUrlInput, sanitized_langs: &[String]) -> SubtitlePlan {
    let mut flags = vec![
        "--skip-download".to_string(),
        "--write-subs".to_string(),
        "--no-playlist".to_string(),
    ];

    // Add auto-subs if allowed
    if input.allow_auto_subs {
        flags.push("--write-auto-subs".to_string());
    }

    // Build language list with live-chat exclusion
    let languages_for_flags = if sanitized_langs.is_empty() {
        default_sub_langs()
    } else {
        sanitized_langs.to_vec()
    };

    let lang_spec = if languages_for_flags.is_empty() {
        // Defensive: should not occur because of default_sub_langs fallback
        "en,-live_chat".to_string()
    } else {
        let langs = languages_for_flags.join(",");
        format!("{},-live_chat", langs)
    };

    flags.push("--sub-langs".to_string());
    flags.push(lang_spec);

    // Output format: SRT is widely compatible
    flags.push("--convert-subs".to_string());
    flags.push("srt".to_string());

    SubtitlePlan {
        enabled: true,
        preferred_languages: sanitized_langs.to_vec(),
        allow_auto_subs: input.allow_auto_subs,
        exclude_live_chat: true,
        yt_dlp_flags: flags,
    }
}

/// Build disabled subtitle plan for unsupported URLs.
fn build_disabled_subtitle_plan() -> SubtitlePlan {
    SubtitlePlan {
        enabled: false,
        preferred_languages: vec![],
        allow_auto_subs: false,
        exclude_live_chat: true,
        yt_dlp_flags: vec![],
    }
}

/// Build video download plan based on URL classification.
fn build_video_download_plan(
    classification: &UrlClassification,
    input: &YoutubeUrlInput,
) -> VideoDownloadPlan {
    let mut flags = vec![
        // Format selection: best video + best audio, fallback to best combined
        "-f".to_string(),
        "bv*[ext=mp4]+ba[ext=m4a]/bv*+ba/b".to_string(),
        // Force MP4 container
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        // Avoid playlist traversal
        "--no-playlist".to_string(),
    ];

    // Add live-specific flags if this is identified as live
    let is_chunked = if classification.is_live {
        match input.live_capture_mode {
            LiveCaptureMode::FromStart => {
                flags.push("--live-from-start".to_string());
            }
            LiveCaptureMode::FromNow => {
                // No special flag needed; yt-dlp captures from current point by default
            }
        }
        true
    } else {
        false
    };

    VideoDownloadPlan {
        enabled: true,
        preferred_container: "mp4".to_string(),
        preferred_codecs: vec!["h264".to_string(), "aac".to_string()],
        is_chunked_live_capture: is_chunked,
        yt_dlp_flags: flags,
    }
}

/// Build disabled video download plan for unsupported URLs.
fn build_disabled_video_plan() -> VideoDownloadPlan {
    VideoDownloadPlan {
        enabled: false,
        preferred_container: "mp4".to_string(),
        preferred_codecs: vec![],
        is_chunked_live_capture: false,
        yt_dlp_flags: vec![],
    }
}

// ============================================================================
// JSON Serialization Helpers
// ============================================================================

impl YoutubeUrlConfig {
    /// Serialize to JSON string.
    ///
    /// Returns a strict JSON object suitable for API responses.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty-printed JSON string.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl YoutubeUrlInput {
    /// Parse from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Analyze a YouTube URL from raw JSON input and return JSON output.
///
/// This is a convenience function for API handlers that receive JSON
/// and need to return JSON.
///
/// # Example JSON Input
/// ```json
/// {
///   "raw_url": "https://youtube.com/watch?v=dQw4w9WgXcQ",
///   "preferred_sub_langs": ["en"],
///   "allow_auto_subs": true,
///   "live_capture_mode": "from_start",
///   "max_expected_duration_sec": 21600
/// }
/// ```
///
/// # Returns
/// A JSON string containing the complete `YoutubeUrlConfig` analysis.
pub fn analyze_youtube_url_json(input_json: &str) -> Result<String, serde_json::Error> {
    let input: YoutubeUrlInput = serde_json::from_str(input_json)?;
    let config = analyze_youtube_url(&input);
    serde_json::to_string(&config)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(url: &str) -> YoutubeUrlInput {
        YoutubeUrlInput {
            raw_url: url.to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        }
    }

    // ========================================================================
    // Valid URL Tests
    // ========================================================================

    #[test]
    fn test_standard_watch_url() {
        let input = make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
        assert_eq!(
            config.normalized_url,
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string())
        );
        assert_eq!(config.url_type, UrlType::Video);
        assert!(!config.is_shorts);
        assert!(!config.is_live);
        assert!(config.validation.errors.is_empty());
    }

    #[test]
    fn test_short_url() {
        let input = make_input("https://youtu.be/dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
        assert_eq!(config.url_type, UrlType::Video);
    }

    #[test]
    fn test_embed_url() {
        let input = make_input("https://www.youtube.com/embed/dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_shorts_url() {
        let input = make_input("https://www.youtube.com/shorts/dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
        assert_eq!(config.url_type, UrlType::Shorts);
        assert!(config.is_shorts);
    }

    #[test]
    fn test_playlist_with_video() {
        let input =
            make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmRdnEQy4qtr");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
        assert_eq!(config.url_type, UrlType::PlaylistItem);
    }

    #[test]
    fn test_url_with_timestamp() {
        let input = make_input("https://youtu.be/dQw4w9WgXcQ?t=30");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_mobile_url() {
        let input = make_input("https://m.youtube.com/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_v_url_format() {
        let input = make_input("https://www.youtube.com/v/dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    // ========================================================================
    // Invalid URL Tests
    // ========================================================================

    #[test]
    fn test_non_youtube_domain() {
        let input = make_input("https://vimeo.com/123456789");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
        assert_eq!(config.url_type, UrlType::Unsupported);
        assert!(config.validation.errors[0].contains("Non-YouTube domain"));
    }

    #[test]
    fn test_youtube_no_video_id() {
        let input = make_input("https://www.youtube.com");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
        assert_eq!(config.url_type, UrlType::Unsupported);
    }

    #[test]
    fn test_playlist_only_no_video() {
        let input = make_input("https://www.youtube.com/playlist?list=PLrAXtmRdnEQy4qtr");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
        assert_eq!(config.url_type, UrlType::Unsupported);
        assert!(config.validation.errors[0].contains("Playlist URL"));
    }

    #[test]
    fn test_channel_no_video() {
        let input = make_input("https://www.youtube.com/@LinusTechTips");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
        assert_eq!(config.url_type, UrlType::Unsupported);
    }

    #[test]
    fn test_invalid_video_id_too_short() {
        let input = make_input("https://www.youtube.com/watch?v=abc123");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
        assert!(config.validation.errors[0].contains("invalid format"));
    }

    #[test]
    fn test_invalid_video_id_bad_chars() {
        let input = make_input("https://www.youtube.com/watch?v=abc123!!xyz");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, None);
    }

    #[test]
    fn test_rejects_youtube_like_but_not_whitelisted_host() {
        let input = make_input("https://evil-youtube.com/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.url_type, UrlType::Unsupported);
        assert!(config
            .validation
            .errors
            .iter()
            .any(|e| e.contains("Non-YouTube domain: evil-youtube.com")));
    }

    #[test]
    fn test_rejects_redirect_with_embedded_youtube_query() {
        let input = make_input(
            "https://example.com/redirect?target=https://youtube.com/watch?v=dQw4w9WgXcQ",
        );
        let config = analyze_youtube_url(&input);

        assert!(!config.validation.is_supported_youtube_url);
        assert_eq!(config.url_type, UrlType::Unsupported);
        assert!(config
            .validation
            .errors
            .iter()
            .any(|e| e.contains("Non-YouTube domain: example.com")));
    }

    // ========================================================================
    // Subtitle Plan Tests
    // ========================================================================

    #[test]
    fn test_subtitle_plan_with_auto_subs() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string(), "es".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert!(config.subtitle_plan.enabled);
        assert!(config.subtitle_plan.allow_auto_subs);
        assert!(config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"--write-auto-subs".to_string()));
        assert!(config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"--skip-download".to_string()));
        assert!(config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"en,es,-live_chat".to_string()));
    }

    #[test]
    fn test_sanitizes_preferred_sub_langs_with_warnings() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec![
                " en ".to_string(),
                "spa ce".to_string(),
                "THISISWAYTOOLONGLANGUAGECODE".to_string(),
                "de-DE".to_string(),
                "quote\"".to_string(),
            ],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(
            config.subtitle_plan.preferred_languages,
            vec!["en".to_string(), "de-de".to_string()]
        );
        assert!(config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"en,de-de,-live_chat".to_string()));
        assert!(config
            .validation
            .errors
            .iter()
            .any(|e| e.contains("Ignored subtitle language 'spa ce'")));
        assert!(config
            .validation
            .errors
            .iter()
            .any(|e| e.contains("too long")));
    }

    #[test]
    fn test_all_invalid_sub_langs_fallback_to_default_flag() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["bad lang".to_string(), "???".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert!(config.subtitle_plan.preferred_languages.is_empty());
        assert!(config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"en,-live_chat".to_string()));
        assert!(config
            .validation
            .errors
            .iter()
            .any(|e| e.contains("defaulting to 'en'")));
    }

    #[test]
    fn test_subtitle_plan_without_auto_subs() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: false,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert!(config.subtitle_plan.enabled);
        assert!(!config.subtitle_plan.allow_auto_subs);
        assert!(!config
            .subtitle_plan
            .yt_dlp_flags
            .contains(&"--write-auto-subs".to_string()));
    }

    // ========================================================================
    // Video Download Plan Tests
    // ========================================================================

    #[test]
    fn test_video_plan_standard() {
        let input = make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.video_download_plan.enabled);
        assert_eq!(config.video_download_plan.preferred_container, "mp4");
        assert!(!config.video_download_plan.is_chunked_live_capture);
        assert!(config
            .video_download_plan
            .yt_dlp_flags
            .contains(&"--no-playlist".to_string()));
        assert!(config
            .video_download_plan
            .yt_dlp_flags
            .contains(&"--merge-output-format".to_string()));
    }

    // ========================================================================
    // Live Handling Tests
    // ========================================================================

    #[test]
    fn test_live_url_classified_as_video_without_live_flags() {
        let input = make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=live");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.url_type, UrlType::Video);
        assert!(!config.is_live);
        assert!(!config.video_download_plan.is_chunked_live_capture);
    }

    #[test]
    fn test_live_handling_from_start() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert_eq!(
            config.live_handling.live_capture_mode,
            LiveCaptureMode::FromStart
        );
        assert!(config.live_handling.download_sections.is_none());
    }

    #[test]
    fn test_live_handling_from_now() {
        let input = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromNow,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input);

        assert_eq!(
            config.live_handling.live_capture_mode,
            LiveCaptureMode::FromNow
        );
    }

    // ========================================================================
    // Normalization Tests
    // ========================================================================

    #[test]
    fn test_normalization_removes_extra_params() {
        let input =
            make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share&si=abc123&t=30");
        let config = analyze_youtube_url(&input);

        assert_eq!(
            config.normalized_url,
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_normalization_from_short_url() {
        let input = make_input("https://youtu.be/dQw4w9WgXcQ?t=30");
        let config = analyze_youtube_url(&input);

        assert_eq!(
            config.normalized_url,
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string())
        );
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_url_with_whitespace() {
        let input = make_input("  https://www.youtube.com/watch?v=dQw4w9WgXcQ  ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_case_insensitive_domain() {
        let input = make_input("https://YOUTUBE.COM/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_youtube_nocookie_domain() {
        let input = make_input("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_disabled_plans_for_unsupported() {
        let input = make_input("https://example.com/video");
        let config = analyze_youtube_url(&input);

        assert!(!config.subtitle_plan.enabled);
        assert!(!config.video_download_plan.enabled);
        assert!(config.subtitle_plan.yt_dlp_flags.is_empty());
        assert!(config.video_download_plan.yt_dlp_flags.is_empty());
    }

    // ========================================================================
    // JSON Serialization Tests
    // ========================================================================

    #[test]
    fn test_json_input_parsing() {
        let json = r#"{
            "raw_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "preferred_sub_langs": ["en", "es"],
            "allow_auto_subs": true,
            "live_capture_mode": "from_start",
            "max_expected_duration_sec": 3600
        }"#;

        let input = YoutubeUrlInput::from_json(json).unwrap();
        assert_eq!(input.raw_url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(input.preferred_sub_langs, vec!["en", "es"]);
        assert!(input.allow_auto_subs);
        assert_eq!(input.live_capture_mode, LiveCaptureMode::FromStart);
        assert_eq!(input.max_expected_duration_sec, 3600);
    }

    #[test]
    fn test_json_input_defaults() {
        let json = r#"{"raw_url": "https://youtu.be/dQw4w9WgXcQ"}"#;

        let input = YoutubeUrlInput::from_json(json).unwrap();
        assert_eq!(input.preferred_sub_langs, vec!["en"]);
        assert!(input.allow_auto_subs);
        assert_eq!(input.live_capture_mode, LiveCaptureMode::FromStart);
        assert_eq!(input.max_expected_duration_sec, 21600);
    }

    #[test]
    fn test_json_output_serialization() {
        let input = make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let config = analyze_youtube_url(&input);

        let json = config.to_json().unwrap();
        assert!(json.contains("dQw4w9WgXcQ"));
        assert!(json.contains("normalized_url"));
        assert!(json.contains("subtitle_plan"));
        assert!(json.contains("video_download_plan"));
    }

    #[test]
    fn test_analyze_youtube_url_json_roundtrip() {
        let input_json = r#"{
            "raw_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "preferred_sub_langs": ["en"],
            "allow_auto_subs": true,
            "live_capture_mode": "from_start",
            "max_expected_duration_sec": 21600
        }"#;

        let output_json = analyze_youtube_url_json(input_json).unwrap();

        // Parse the output to verify structure
        let config: YoutubeUrlConfig = serde_json::from_str(&output_json).unwrap();
        assert!(config.validation.is_supported_youtube_url);
        assert_eq!(config.video_id, Some("dQw4w9WgXcQ".to_string()));
        assert_eq!(config.url_type, UrlType::Video);
    }

    #[test]
    fn test_json_url_type_serialization() {
        // Test each URL type serializes correctly
        let shorts_input = make_input("https://www.youtube.com/shorts/dQw4w9WgXcQ");
        let shorts_config = analyze_youtube_url(&shorts_input);
        let shorts_json = shorts_config.to_json().unwrap();
        assert!(shorts_json.contains(r#""url_type":"shorts""#));

        let video_input = make_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let video_config = analyze_youtube_url(&video_input);
        let video_json = video_config.to_json().unwrap();
        assert!(video_json.contains(r#""url_type":"video""#));

        let unsupported_input = make_input("https://example.com/video");
        let unsupported_config = analyze_youtube_url(&unsupported_input);
        let unsupported_json = unsupported_config.to_json().unwrap();
        assert!(unsupported_json.contains(r#""url_type":"unsupported""#));
    }

    #[test]
    fn test_json_live_capture_mode_serialization() {
        let input_from_start = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input_from_start);
        let json = config.to_json().unwrap();
        assert!(json.contains(r#""live_capture_mode":"from_start""#));

        let input_from_now = YoutubeUrlInput {
            raw_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromNow,
            max_expected_duration_sec: 21600,
        };
        let config = analyze_youtube_url(&input_from_now);
        let json = config.to_json().unwrap();
        assert!(json.contains(r#""live_capture_mode":"from_now""#));
    }
}
