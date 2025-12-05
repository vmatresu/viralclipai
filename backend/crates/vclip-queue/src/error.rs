//! Queue error types.

use thiserror::Error;

pub type QueueResult<T> = Result<T, QueueError>;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Enqueue failed: {0}")]
    EnqueueFailed(String),

    #[error("Dequeue failed: {0}")]
    DequeueFailed(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl QueueError {
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ConnectionFailed(msg.into())
    }

    pub fn enqueue_failed(msg: impl Into<String>) -> Self {
        Self::EnqueueFailed(msg.into())
    }
}
