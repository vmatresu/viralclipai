//! Video download using yt-dlp.
//!
//! This module provides functions to download videos and segments from YouTube
//! and other platforms using yt-dlp. Includes IPv6 rotation support for
//! avoiding rate limiting.

use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::{MediaError, MediaResult};
use crate::ipv6_rotation::{get_random_ipv6_address, record_ipv6_failure, record_ipv6_success};

/// Minimum video file size threshold (50MB) to consider download complete.
const MIN_VIDEO_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Source cookies path (mounted read-only in Docker).
const SOURCE_COOKIES_PATH: &str = "/app/youtube-cookies.txt";

/// Writable temp path for cookies (allows yt-dlp to save cookies).
const TEMP_COOKIES_PATH: &str = "/tmp/youtube-cookies.txt";

/// Minimum size for a valid cookies file (bytes).
/// A real Netscape cookies file is at least ~50 bytes.
const MIN_COOKIES_FILE_SIZE: u64 = 50;

/// Guards concurrent access to cookies file copy.
static COOKIES_LOCK: OnceLock<Mutex<bool>> = OnceLock::new();

/// Validate that a cookies file appears to be in Netscape format.
///
/// Netscape cookies files either start with "# Netscape HTTP Cookie File"
/// or contain tab-separated lines with domain entries.
fn is_valid_netscape_cookies(content: &str) -> bool {
    // Check for Netscape header
    if content.starts_with("# Netscape HTTP Cookie File")
        || content.starts_with("# HTTP Cookie File")
    {
        return true;
    }

    // Check for tab-separated cookie entries (domain\ttrue/false\t...)
    for line in content.lines() {
        let line = line.trim();
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Valid cookie line should have tab-separated fields
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 6 {
            return true;
        }
    }

    false
}

/// Get path to a writable cookies file.
///
/// Copies the source cookies file to a temp location if needed,
/// since yt-dlp tries to save cookies back after use.
///
/// Returns `None` if:
/// - The file doesn't exist
/// - The file is empty or too small
/// - The file is not in valid Netscape format
pub async fn get_writable_cookies_path() -> Option<String> {
    let source_path = Path::new(SOURCE_COOKIES_PATH);

    // Check if file exists
    if !source_path.exists() {
        debug!(
            "Cookies file not found at {}, skipping",
            SOURCE_COOKIES_PATH
        );
        return None;
    }

    // Check file size - empty or tiny files are invalid
    match tokio::fs::metadata(source_path).await {
        Ok(metadata) => {
            if metadata.len() < MIN_COOKIES_FILE_SIZE {
                debug!(
                    "Cookies file {} is too small ({} bytes), skipping",
                    SOURCE_COOKIES_PATH,
                    metadata.len()
                );
                return None;
            }
        }
        Err(e) => {
            warn!("Failed to read cookies file metadata: {}", e);
            return None;
        }
    }

    // Validate Netscape format
    match tokio::fs::read_to_string(source_path).await {
        Ok(content) => {
            if !is_valid_netscape_cookies(&content) {
                debug!(
                    "Cookies file {} is not in valid Netscape format, skipping",
                    SOURCE_COOKIES_PATH
                );
                return None;
            }
        }
        Err(e) => {
            warn!("Failed to read cookies file: {}", e);
            return None;
        }
    }

    let temp_path = Path::new(TEMP_COOKIES_PATH);
    let lock = COOKIES_LOCK.get_or_init(|| Mutex::new(false));

    let mut copied = lock.lock().await;

    // Copy if not yet copied or temp file doesn't exist
    if !*copied || !temp_path.exists() {
        match tokio::fs::copy(source_path, temp_path).await {
            Ok(_) => {
                debug!(
                    "Copied cookies file to writable location: {}",
                    TEMP_COOKIES_PATH
                );
                *copied = true;
            }
            Err(e) => {
                warn!("Failed to copy cookies file to temp: {}", e);
                return None;
            }
        }
    }

    info!("Using cookies file for YouTube authentication");
    Some(TEMP_COOKIES_PATH.to_string())
}

/// Download a video from URL using yt-dlp.
///
/// # Features
///
/// - IPv6 rotation to avoid rate limiting
/// - Cookie-based authentication for YouTube
/// - Automatic retry on transient failures
/// - Sleep intervals to avoid detection
///
/// # Arguments
///
/// * `url` - Video URL (YouTube, Vimeo, etc.)
/// * `output_path` - Path to save the downloaded video
///
/// # Returns
///
/// - `Ok(())` if download was successful
/// - `Err(MediaError)` on failure
pub async fn download_video(url: &str, output_path: impl AsRef<Path>) -> MediaResult<()> {
    let output_path = output_path.as_ref();

    // Check if file already exists and is large enough
    if output_path.exists() {
        if let Ok(metadata) = output_path.metadata() {
            if metadata.len() > MIN_VIDEO_FILE_SIZE {
                info!("Using existing video file: {}", output_path.display());
                return Ok(());
            }
            warn!(
                "Existing file {} is too small ({} bytes), re-downloading",
                output_path.display(),
                metadata.len()
            );
            tokio::fs::remove_file(output_path).await?;
        }
    }

    // Check yt-dlp exists
    which::which("yt-dlp").map_err(|_| MediaError::YtDlpNotFound)?;

    info!(
        "Downloading video from {} to {}",
        url,
        output_path.display()
    );

    // Use cookies file if available for YouTube authentication (copy to writable location)
    let cookies_path = get_writable_cookies_path().await;
    let output_path_str = output_path.to_string_lossy();

    let mut args = vec![
        "--verbose",
        "--remote-components", "ejs:github",
        "--sleep-subtitles", "5",
        "--sleep-requests", "0.75", 
        "--sleep-interval", "10",
        "--max-sleep-interval", "20",
        "--user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "--add-header", "Accept:text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "--add-header", "Accept-Language:en-US,en;q=0.5",
        "--add-header", "Accept-Encoding:gzip, deflate",
        "--add-header", "DNT:1",
        "--add-header", "Connection:keep-alive",
        "--add-header", "Upgrade-Insecure-Requests:1",
        "--limit-rate", "2M",
        "--concurrent-fragments", "1",
        "--extractor-args", "youtube:player_client=web",
        "--force-ipv6",
        "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
        "-o",
    ];

    args.push(&output_path_str);

    // IPv6 rotation: select random source address if available
    // Uses cached address pool from ipv6_rotation module
    let ipv6_source = get_random_ipv6_address();
    let ipv6_ref = ipv6_source.as_deref();
    if let Some(ip) = ipv6_ref {
        args.push("--source-address");
        args.push(ip);
        info!(ipv6_address = %ip, "Using IPv6 source address for download");
    }

    let cookies_ref = cookies_path.as_deref();
    if let Some(cp) = cookies_ref {
        args.push("--cookies");
        args.push(cp);
    }
    args.push(url);

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    let using_ipv6 = ipv6_source.is_some();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("yt-dlp stderr: {}", stderr);

        // Record failure for IPv6 metrics
        if using_ipv6 {
            record_ipv6_failure();
        }

        // Detect rate limiting for better error messages
        let error_msg = stderr.lines().last().unwrap_or("Unknown error");
        let is_rate_limited = stderr.contains("429")
            || stderr.contains("Too Many Requests")
            || stderr.contains("rate limit")
            || stderr.contains("Sign in to confirm");

        if is_rate_limited {
            warn!(
                url = %url,
                using_ipv6 = using_ipv6,
                "YouTube rate limit detected"
            );
        }

        return Err(MediaError::download_failed(format!(
            "yt-dlp failed: {}",
            error_msg
        )));
    }

    // Verify file was created
    if !output_path.exists() {
        if using_ipv6 {
            record_ipv6_failure();
        }
        return Err(MediaError::download_failed("Output file not created"));
    }

    // Record success for IPv6 metrics
    if using_ipv6 {
        record_ipv6_success();
    }

    let file_size = output_path.metadata()?.len();
    info!(
        output = %output_path.display(),
        size_mb = file_size as f64 / (1024.0 * 1024.0),
        using_ipv6 = using_ipv6,
        "Downloaded video successfully"
    );

    Ok(())
}

/// Check if a URL is a supported video platform.
pub fn is_supported_url(url: &str) -> bool {
    let supported_domains = [
        "youtube.com",
        "youtu.be",
        "vimeo.com",
        "twitter.com",
        "x.com",
        "twitch.tv",
        "tiktok.com",
    ];

    supported_domains.iter().any(|domain| url.contains(domain))
}

/// Error indicating segment download is not supported for this source.
#[derive(Debug)]
pub struct SegmentDownloadNotSupported {
    pub reason: String,
}

impl std::fmt::Display for SegmentDownloadNotSupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Segment download not supported: {}", self.reason)
    }
}

impl std::error::Error for SegmentDownloadNotSupported {}

/// Download a specific segment from a video using yt-dlp `--download-sections`.
///
/// This is more efficient than downloading the full video and then trimming.
/// Works with HLS streams; may fail for DASH-only sources (returns error to allow fallback).
///
/// # Arguments
/// * `url` - Video URL (YouTube, etc.)
/// * `start_secs` - Start time in seconds
/// * `end_secs` - End time in seconds
/// * `output_path` - Path to save the segment
/// * `force_keyframes` - If true, use `--force-keyframes-at-cuts` for accurate cuts (slower, re-encodes)
///
/// # Returns
/// * `Ok(())` if segment was downloaded successfully
/// * `Err(SegmentDownloadNotSupported)` if the source doesn't support segment downloads (DASH-only)
/// * `Err(MediaError)` for other download failures
pub async fn download_segment(
    url: &str,
    start_secs: f64,
    end_secs: f64,
    output_path: impl AsRef<Path>,
    force_keyframes: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output_path = output_path.as_ref();

    // Check yt-dlp exists
    which::which("yt-dlp").map_err(|_| MediaError::YtDlpNotFound)?;

    // Build section argument: "*start-end" format
    let section_arg = format!("*{:.0}-{:.0}", start_secs, end_secs);

    info!(
        url = url,
        start = start_secs,
        end = end_secs,
        output = %output_path.display(),
        "Attempting segment download with yt-dlp --download-sections"
    );

    // Use cookies file if available for YouTube authentication (copy to writable location)
    let cookies_path = get_writable_cookies_path().await;
    let output_path_str = output_path.to_string_lossy();

    let mut args = vec![
        "--remote-components".to_string(),
        "ejs:github".to_string(),
        "--sleep-subtitles".to_string(),
        "5".to_string(),
        "--sleep-requests".to_string(),
        "0.75".to_string(),
        "--sleep-interval".to_string(),
        "10".to_string(),
        "--max-sleep-interval".to_string(),
        "20".to_string(),
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        "--add-header".to_string(),
        "Accept:text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".to_string(),
        "--add-header".to_string(),
        "Accept-Language:en-US,en;q=0.5".to_string(),
        "--add-header".to_string(),
        "Accept-Encoding:gzip, deflate".to_string(),
        "--add-header".to_string(),
        "DNT:1".to_string(),
        "--add-header".to_string(),
        "Connection:keep-alive".to_string(),
        "--add-header".to_string(),
        "Upgrade-Insecure-Requests:1".to_string(),
        "--limit-rate".to_string(),
        "2M".to_string(),
        "--concurrent-fragments".to_string(),
        "1".to_string(),
        "--extractor-args".to_string(),
        "youtube:player_client=web".to_string(),
        "--force-ipv6".to_string(),
        "--download-sections".to_string(),
        section_arg,
        // Prefer HLS format which supports segment downloads
        "-f".to_string(),
        "bestvideo[ext=mp4][protocol=m3u8_native]+bestaudio[ext=m4a][protocol=m3u8_native]/bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best".to_string(),
        "-o".to_string(),
        output_path_str.to_string(),
    ];

    // Add force-keyframes for accurate cuts (re-encodes, slower but more accurate)
    if force_keyframes {
        args.push("--force-keyframes-at-cuts".to_string());
    }

    // IPv6 rotation: select random source address if available
    // Uses cached address pool from ipv6_rotation module
    let ipv6_source = get_random_ipv6_address();
    let using_ipv6 = ipv6_source.is_some();
    if let Some(ip) = &ipv6_source {
        args.push("--source-address".to_string());
        args.push(ip.clone());
        info!(ipv6_address = %ip, "Using IPv6 source address for segment download");
    }

    if let Some(cp) = &cookies_path {
        args.push("--cookies".to_string());
        args.push(cp.clone());
    }
    args.push(url.to_string());

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("yt-dlp segment download stderr: {}", stderr);

        // Record failure for IPv6 metrics
        if using_ipv6 {
            record_ipv6_failure();
        }

        // Check if failure is due to DASH-only or unsupported segment download
        if stderr.contains("--download-sections")
            || stderr.contains("does not support")
            || stderr.contains("DASH")
            || stderr.contains("Unable to download section")
        {
            return Err(Box::new(SegmentDownloadNotSupported {
                reason: format!(
                    "Source may not support HLS segment downloads: {}",
                    stderr.lines().last().unwrap_or("Unknown error")
                ),
            }));
        }

        return Err(Box::new(MediaError::download_failed(format!(
            "yt-dlp segment download failed: {}",
            stderr.lines().last().unwrap_or("Unknown error")
        ))));
    }

    // Verify file was created
    if !output_path.exists() {
        if using_ipv6 {
            record_ipv6_failure();
        }
        return Err(Box::new(MediaError::download_failed(
            "Segment output file not created",
        )));
    }

    // Record success for IPv6 metrics
    if using_ipv6 {
        record_ipv6_success();
    }

    let file_size = output_path.metadata()?.len();
    info!(
        output = %output_path.display(),
        size_mb = file_size as f64 / (1024.0 * 1024.0),
        using_ipv6 = using_ipv6,
        "Downloaded video segment successfully"
    );

    Ok(())
}

/// Check if a URL likely supports segment downloads (HLS).
///
/// This is a heuristic check - actual support depends on the video.
/// YouTube typically supports HLS for most videos.
pub fn likely_supports_segment_download(url: &str) -> bool {
    // YouTube generally supports HLS
    url.contains("youtube.com") || url.contains("youtu.be")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::extract_youtube_id;

    #[test]
    fn test_is_supported_url() {
        assert!(is_supported_url("https://youtube.com/watch?v=abc"));
        assert!(is_supported_url("https://youtu.be/abc"));
        assert!(is_supported_url("https://vimeo.com/123"));
        assert!(!is_supported_url("https://example.com/video"));
    }

    #[test]
    fn test_extract_youtube_id() {
        use vclip_models::YoutubeIdError;

        // Standard youtube.com format
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def45"),
            Ok("abc123def45".to_string())
        );

        // youtu.be format
        assert_eq!(
            extract_youtube_id("https://youtu.be/abc123def45"),
            Ok("abc123def45".to_string())
        );

        // With query parameters
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def45&list=xyz"),
            Ok("abc123def45".to_string())
        );

        // Embed format
        assert_eq!(
            extract_youtube_id("https://youtube.com/embed/abc123def45"),
            Ok("abc123def45".to_string())
        );

        // Invalid formats
        assert_eq!(
            extract_youtube_id("https://example.com"),
            Err(YoutubeIdError::InvalidYoutubeUrl)
        );
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch"),
            Err(YoutubeIdError::VideoIdNotFound)
        );
        assert_eq!(
            extract_youtube_id("https://youtu.be/"),
            Err(YoutubeIdError::VideoIdNotFound)
        );

        // Invalid video ID format (wrong length)
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123"),
            Err(YoutubeIdError::InvalidVideoId)
        );

        // Invalid video ID format (invalid characters)
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def!!"),
            Err(YoutubeIdError::InvalidVideoId)
        );
    }
}
