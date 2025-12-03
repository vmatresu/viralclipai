//! Health check handlers.

use axum::Json;
use chrono::Utc;
use serde::Serialize;

/// Health response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now().to_rfc3339(),
    })
}

/// Readiness check endpoint.
pub async fn ready() -> Json<serde_json::Value> {
    // TODO: Add dependency checks (Redis, Firestore, R2)
    Json(serde_json::json!({ "status": "ready" }))
}
