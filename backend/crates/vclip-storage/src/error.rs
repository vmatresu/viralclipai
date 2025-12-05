//! Storage error types.

use thiserror::Error;

/// Result type for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Failed to configure storage client: {0}")]
    ConfigError(String),

    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Delete failed: {0}")]
    DeleteFailed(String),

    #[error("List failed: {0}")]
    ListFailed(String),

    #[error("Presign failed: {0}")]
    PresignFailed(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("AWS SDK error: {0}")]
    AwsSdk(String),
}

impl StorageError {
    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    pub fn not_found(key: impl Into<String>) -> Self {
        Self::NotFound(key.into())
    }

    pub fn upload_failed(msg: impl Into<String>) -> Self {
        Self::UploadFailed(msg.into())
    }

    pub fn delete_failed(msg: impl Into<String>) -> Self {
        Self::DeleteFailed(msg.into())
    }
}
