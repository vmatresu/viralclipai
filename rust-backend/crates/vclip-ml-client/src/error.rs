//! ML client error types.

use thiserror::Error;

pub type MlResult<T> = Result<T, MlError>;

#[derive(Debug, Error)]
pub enum MlError {
    #[error("ML service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Timeout after {0} seconds")]
    Timeout(u64),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl MlError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            MlError::ServiceUnavailable(_) | MlError::Timeout(_) | MlError::Network(_)
        )
    }
}
