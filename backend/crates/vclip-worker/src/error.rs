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

    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Queue operation failed: {0}")]
    QueueFailed(String),

    #[error("Reschedule: {0}")]
    Reschedule(String),

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

    pub fn quota_exceeded(msg: impl Into<String>) -> Self {
        Self::QuotaExceeded(msg.into())
    }

    pub fn queue_failed(msg: impl Into<String>) -> Self {
        Self::QueueFailed(msg.into())
    }

    /// Create a reschedule error - indicates the job should be retried later.
    ///
    /// Used for the analysis-first pattern where processing must wait
    /// for analysis to complete.
    pub fn reschedule(msg: impl Into<String>) -> Self {
        Self::Reschedule(msg.into())
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

    /// Check if error is a reschedule request (analysis pending).
    pub fn is_reschedule(&self) -> bool {
        matches!(self, WorkerError::Reschedule(_))
    }

    /// Check if error is a quota exceeded error (not retryable, user action needed).
    pub fn is_quota_exceeded(&self) -> bool {
        matches!(self, WorkerError::QuotaExceeded(_))
    }

    /// Check if this is a permanent failure that should NOT be retried.
    ///
    /// These are errors where retrying won't help - the video itself is
    /// inaccessible (age-restricted, private, unavailable, etc.).
    /// The job should immediately fail so the user sees the error.
    pub fn is_permanent_failure(&self) -> bool {
        // Quota exceeded is permanent
        if self.is_quota_exceeded() {
            return true;
        }

        // Check the error message for permanent failure patterns
        let msg = self.to_string().to_lowercase();

        // Age restriction (requires login/cookies we don't have)
        if msg.contains("age") && (msg.contains("restrict") || msg.contains("verif")) {
            return true;
        }

        // Private videos
        if msg.contains("private video") || msg.contains("video is private") {
            return true;
        }

        // Unavailable videos
        if msg.contains("video unavailable")
            || msg.contains("video is unavailable")
            || msg.contains("video not available")
        {
            return true;
        }

        // Deleted videos
        if msg.contains("video has been removed") || msg.contains("video was deleted") {
            return true;
        }

        // Copyright blocked
        if msg.contains("copyright") && msg.contains("block") {
            return true;
        }

        // Region blocked
        if msg.contains("not available in your country") || msg.contains("blocked in your country")
        {
            return true;
        }

        // Live streams (can't process)
        if msg.contains("live stream") || msg.contains("live event") {
            return true;
        }

        // Premieres (not yet available)
        if msg.contains("premiere") && msg.contains("will begin") {
            return true;
        }

        false
    }
}
