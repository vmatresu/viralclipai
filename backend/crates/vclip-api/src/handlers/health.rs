//! Health check handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Serialize;

use crate::state::AppState;

/// Health response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
}

/// Health check endpoint (liveness probe).
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now().to_rfc3339(),
    })
}

/// Readiness check response.
#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub checks: ReadinessChecks,
}

#[derive(Serialize)]
pub struct ReadinessChecks {
    pub redis: CheckStatus,
    pub firestore: CheckStatus,
    pub storage: CheckStatus,
}

#[derive(Serialize)]
pub struct CheckStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

impl CheckStatus {
    fn ok(latency_ms: u64) -> Self {
        Self {
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(latency_ms),
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            error: Some(msg.into()),
            latency_ms: None,
        }
    }
}

/// Readiness check endpoint (readiness probe).
/// Checks connectivity to Redis, Firestore, and R2.
pub async fn ready(
    State(state): State<AppState>,
) -> Result<Json<ReadinessResponse>, (StatusCode, Json<ReadinessResponse>)> {
    use std::time::Instant;

    // Check Redis (via queue length)
    let redis_check = {
        let start = Instant::now();
        match state.queue.len().await {
            Ok(_) => CheckStatus::ok(start.elapsed().as_millis() as u64),
            Err(e) => CheckStatus::error(e.to_string()),
        }
    };

    // Check Firestore (via simple document read)
    let firestore_check = {
        let start = Instant::now();
        match state.firestore.get_document("_health", "_check").await {
            Ok(_) => CheckStatus::ok(start.elapsed().as_millis() as u64),
            Err(e) => {
                // NotFound is OK - it means Firestore is reachable
                if e.to_string().contains("NOT_FOUND") || e.to_string().contains("404") {
                    CheckStatus::ok(start.elapsed().as_millis() as u64)
                } else {
                    CheckStatus::error(e.to_string())
                }
            }
        }
    };

    // Check R2 storage (via bucket head)
    let storage_check = {
        let start = Instant::now();
        match state.storage.check_connectivity().await {
            Ok(_) => CheckStatus::ok(start.elapsed().as_millis() as u64),
            Err(e) => CheckStatus::error(e.to_string()),
        }
    };

    let all_ok = redis_check.status == "ok"
        && firestore_check.status == "ok"
        && storage_check.status == "ok";

    let response = ReadinessResponse {
        status: if all_ok { "ready" } else { "degraded" }.to_string(),
        checks: ReadinessChecks {
            redis: redis_check,
            firestore: firestore_check,
            storage: storage_check,
        },
    };

    if all_ok {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}
