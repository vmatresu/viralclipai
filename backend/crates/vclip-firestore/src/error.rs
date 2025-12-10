//! Firestore error types.

use thiserror::Error;

/// Result type for Firestore operations.
pub type FirestoreResult<T> = Result<T, FirestoreError>;

/// Errors that can occur during Firestore operations.
#[derive(Debug, Error)]
pub enum FirestoreError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Document not found: {0}")]
    NotFound(String),

    #[error("Document already exists: {0}")]
    AlreadyExists(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Rate limited, retry after {0}ms")]
    RateLimited(u64),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),
}

impl FirestoreError {
    pub fn auth_error(msg: impl Into<String>) -> Self {
        Self::AuthError(msg.into())
    }

    pub fn not_found(path: impl Into<String>) -> Self {
        Self::NotFound(path.into())
    }

    pub fn request_failed(msg: impl Into<String>) -> Self {
        Self::RequestFailed(msg.into())
    }

    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FirestoreError::Network(_) | FirestoreError::RateLimited(_)
        )
    }

    /// True if the error was caused by a failed precondition (e.g., updateTime mismatch).
    pub fn is_precondition_failed(&self) -> bool {
        matches!(self, FirestoreError::PreconditionFailed(_))
            || matches!(
                self,
                FirestoreError::RequestFailed(msg)
                if msg.contains("FAILED_PRECONDITION") || msg.contains("Precondition")
            )
    }
}
