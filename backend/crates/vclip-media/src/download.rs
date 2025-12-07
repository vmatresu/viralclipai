//! Video download using yt-dlp.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::error::{MediaError, MediaResult};

/// Minimum video file size threshold (50MB) to consider download complete.
const MIN_VIDEO_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Download a video from URL using yt-dlp.
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

    info!("Downloading video from {} to {}", url, output_path.display());

    let output = Command::new("yt-dlp")
        .args([
            "--remote-components", "ejs:github",
            "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
            "-o",
        ])
        .arg(output_path)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("yt-dlp stderr: {}", stderr);
        return Err(MediaError::download_failed(format!(
            "yt-dlp failed: {}",
            stderr.lines().last().unwrap_or("Unknown error")
        )));
    }

    // Verify file was created
    if !output_path.exists() {
        return Err(MediaError::download_failed("Output file not created"));
    }

    let file_size = output_path.metadata()?.len();
    info!(
        "Downloaded video: {} ({:.1} MB)",
        output_path.display(),
        file_size as f64 / (1024.0 * 1024.0)
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
        // Standard youtube.com format
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def45"),
            Some("abc123def45".to_string())
        );

        // youtu.be format
        assert_eq!(
            extract_youtube_id("https://youtu.be/abc123def45"),
            Some("abc123def45".to_string())
        );

        // With query parameters
        assert_eq!(
            extract_youtube_id("https://youtube.com/watch?v=abc123def45&list=xyz"),
            Some("abc123def45".to_string())
        );

        // Embed format
        assert_eq!(
            extract_youtube_id("https://youtube.com/embed/abc123def45"),
            Some("abc123def45".to_string())
        );

        // Invalid formats
        assert_eq!(extract_youtube_id("https://example.com"), None);
        assert_eq!(extract_youtube_id("https://youtube.com/watch"), None);
        assert_eq!(extract_youtube_id("https://youtu.be/"), None);

        // Invalid video ID format (wrong length)
        assert_eq!(extract_youtube_id("https://youtube.com/watch?v=abc123"), None);

        // Invalid video ID format (invalid characters)
        assert_eq!(extract_youtube_id("https://youtube.com/watch?v=abc123def!!"), None);
    }
}
