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

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Face detection failed: {0}")]
    DetectionFailed(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl MediaError {
    /// Create a detection failure error.
    pub fn detection_failed(message: impl Into<String>) -> Self {
        Self::DetectionFailed(message.into())
    }

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

    /// Create a model not found error.
    pub fn model_not_found(path: impl Into<String>) -> Self {
        Self::ModelNotFound(path.into())
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}
