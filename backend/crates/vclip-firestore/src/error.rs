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

    #[error("Server error ({0}): {1}")]
    ServerError(u16, String),

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

    /// Create appropriate error from HTTP status code.
    pub fn from_http_status(status: u16, message: impl Into<String>) -> Self {
        let msg = message.into();
        match status {
            401 => Self::AuthError(msg),
            403 => Self::PermissionDenied(msg),
            404 => Self::NotFound(msg),
            409 => Self::AlreadyExists(msg),
            412 => Self::PreconditionFailed(msg),
            429 => Self::RateLimited(1000), // Default 1 second
            500..=599 => Self::ServerError(status, msg),
            _ => Self::RequestFailed(msg),
        }
    }

    /// Check if error is retryable.
    ///
    /// Retryable errors include:
    /// - Network errors (connection issues, timeouts)
    /// - Rate limited (429)
    /// - Server errors (5xx)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FirestoreError::Network(_)
                | FirestoreError::RateLimited(_)
                | FirestoreError::ServerError(_, _)
        )
    }

    /// Get HTTP status code if available.
    pub fn http_status(&self) -> Option<u16> {
        match self {
            FirestoreError::AuthError(_) => Some(401),
            FirestoreError::RateLimited(_) => Some(429),
            FirestoreError::ServerError(status, _) => Some(*status),
            FirestoreError::NotFound(_) => Some(404),
            FirestoreError::AlreadyExists(_) => Some(409),
            FirestoreError::PermissionDenied(_) => Some(403),
            FirestoreError::PreconditionFailed(_) => Some(412),
            FirestoreError::Network(e) => e.status().map(|s| s.as_u16()),
            _ => None,
        }
    }

    /// Get Retry-After hint in milliseconds if available.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            FirestoreError::RateLimited(ms) => Some(*ms),
            _ => None,
        }
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
