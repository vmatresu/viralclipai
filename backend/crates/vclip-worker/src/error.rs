//! Worker error types.

use thiserror::Error;

pub type WorkerResult<T> = Result<T, WorkerError>;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Processing failed: {0}")]
    ProcessingFailed(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("AI analysis failed: {0}")]
    AiFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Storage error: {0}")]
    Storage(#[from] vclip_storage::StorageError),

    #[error("Firestore error: {0}")]
    Firestore(#[from] vclip_firestore::FirestoreError),

    #[error("Media error: {0}")]
    Media(#[from] vclip_media::MediaError),

    #[error("Queue error: {0}")]
    Queue(#[from] vclip_queue::QueueError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl WorkerError {
    pub fn job_failed(msg: impl Into<String>) -> Self {
        Self::JobFailed(msg.into())
    }

    pub fn processing_failed(msg: impl Into<String>) -> Self {
        Self::ProcessingFailed(msg.into())
    }

    pub fn ai_failed(msg: impl Into<String>) -> Self {
        Self::AiFailed(msg.into())
    }

    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            WorkerError::DownloadFailed(_)
                | WorkerError::UploadFailed(_)
                | WorkerError::Storage(_)
                | WorkerError::Firestore(_)
                | WorkerError::AiFailed(_)
        )
    }
}
