//! API error types.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Gone: {0}")]
    Gone(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Storage error: {0}")]
    Storage(#[from] vclip_storage::StorageError),

    #[error("Firestore error: {0}")]
    Firestore(#[from] vclip_firestore::FirestoreError),

    #[error("Queue error: {0}")]
    Queue(#[from] vclip_queue::QueueError),
}

impl ApiError {
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Gone(_) => StatusCode::GONE,
            ApiError::BadRequest(_) | ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal(_) | ApiError::Storage(_) | ApiError::Firestore(_) | ApiError::Queue(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        // Don't expose internal error details in production
        let detail = match &self {
            ApiError::Internal(_) | ApiError::Storage(_) | ApiError::Firestore(_) | ApiError::Queue(_) => {
                if std::env::var("ENVIRONMENT").unwrap_or_default() == "production" {
                    "An internal error occurred".to_string()
                } else {
                    self.to_string()
                }
            }
            _ => self.to_string(),
        };

        let body = ErrorResponse { detail, code: None };

        (status, Json(body)).into_response()
    }
}
