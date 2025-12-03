//! Error types for media operations.

use std::path::PathBuf;
use thiserror::Error;

/// Result type for media operations.
pub type MediaResult<T> = Result<T, MediaError>;

/// Errors that can occur during media processing.
#[derive(Debug, Error)]
pub enum MediaError {
    #[error("FFmpeg not found in PATH")]
    FfmpegNotFound,

    #[error("FFprobe not found in PATH")]
    FfprobeNotFound,

    #[error("yt-dlp not found in PATH")]
    YtDlpNotFound,

    #[error("FFmpeg command failed: {message}")]
    FfmpegFailed {
        message: String,
        stderr: Option<String>,
        exit_code: Option<i32>,
    },

    #[error("FFprobe command failed: {message}")]
    FfprobeFailed {
        message: String,
        stderr: Option<String>,
    },

    #[error("Download failed: {message}")]
    DownloadFailed { message: String },

    #[error("Invalid timestamp format: {0}")]
    InvalidTimestamp(String),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Operation timed out after {0} seconds")]
    Timeout(u64),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Invalid video file: {0}")]
    InvalidVideo(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

impl MediaError {
    /// Create an FFmpeg failure error.
    pub fn ffmpeg_failed(
        message: impl Into<String>,
        stderr: Option<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self::FfmpegFailed {
            message: message.into(),
            stderr,
            exit_code,
        }
    }

    /// Create a download failure error.
    pub fn download_failed(message: impl Into<String>) -> Self {
        Self::DownloadFailed {
            message: message.into(),
        }
    }
}
