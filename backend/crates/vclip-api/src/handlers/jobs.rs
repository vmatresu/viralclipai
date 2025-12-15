//! Job status handlers for progress polling.
//!
//! Provides REST API endpoints for:
//! - Getting job status (for polling fallback when WebSocket disconnects)
//! - Getting progress history (for recovery after reconnect)

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use vclip_models::JobId;
use vclip_queue::{STALE_GRACE_PERIOD_SECS, STALE_THRESHOLD_SECS};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Query parameters for job status endpoint.
#[derive(Debug, Deserialize)]
pub struct GetJobStatusQuery {
    /// Get events since this timestamp (milliseconds since epoch).
    /// If provided, only returns events newer than this timestamp.
    #[serde(default)]
    pub since: Option<i64>,

    /// Include full event history in response.
    #[serde(default)]
    pub include_history: bool,
}

/// Job status response.
#[derive(Debug, Serialize)]
pub struct JobStatusResponse {
    /// Job ID
    pub job_id: String,
    /// Associated video ID
    pub video_id: String,
    /// Current status: queued, processing, completed, failed, stale
    pub status: String,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Number of clips completed
    pub clips_completed: u32,
    /// Total number of clips to process
    pub clips_total: u32,
    /// Current processing step description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    /// Error message if job failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// When the job was started
    pub started_at: String,
    /// When the status was last updated
    pub updated_at: String,
    /// Last heartbeat from worker (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<String>,
    /// Whether the job appears to be stale (worker may have crashed)
    pub is_stale: bool,
    /// Recent progress events (if include_history=true or since provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<serde_json::Value>>,
    /// Event sequence number for client synchronization
    pub event_seq: u64,
}

/// Progress event in history response.
#[derive(Debug, Serialize)]
pub struct ProgressEventResponse {
    /// Event type
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event timestamp
    pub timestamp_ms: i64,
    /// Event sequence number
    pub seq: u64,
    /// Event-specific data
    #[serde(flatten)]
    pub data: serde_json::Value,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/jobs/:job_id/status
///
/// Get the current status of a processing job.
///
/// This endpoint is used as a polling fallback when WebSocket connection is unavailable
/// or for recovery after a page refresh.
///
/// Query parameters:
/// - `since`: Get events since this timestamp (ms)
/// - `include_history`: Include full event history
///
/// Returns:
/// - 200: Job status with optional event history
/// - 401: Not authenticated
/// - 403: Job belongs to another user
/// - 404: Job not found
pub async fn get_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<GetJobStatusQuery>,
    user: AuthUser,
) -> ApiResult<Json<JobStatusResponse>> {
    info!(
        "get_job_status uid={} job_id={} since={:?} include_history={}",
        user.uid, job_id, query.since, query.include_history
    );

    // Validate job ID format
    if !is_valid_job_id(&job_id) {
        return Err(ApiError::bad_request("Invalid job ID format"));
    }

    let job_id_typed = JobId::from(job_id.clone());

    // Get cached status from Redis
    let status = state
        .progress
        .get_job_status(&job_id_typed)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get job status: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Job not found"))?;

    // Verify ownership
    if status.user_id != user.uid {
        return Err(ApiError::forbidden("Access denied"));
    }

    // Check if stale (no heartbeat for > threshold and not terminal)
    let is_stale = !status.is_terminal()
        && status.is_stale(STALE_THRESHOLD_SECS, STALE_GRACE_PERIOD_SECS);

    // Get event history if requested
    let events = if query.include_history || query.since.is_some() {
        let since = query.since.unwrap_or(0);
        let history = state
            .progress
            .get_history_since(&job_id_typed, since)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?;

        Some(
            history
                .into_iter()
                .filter_map(|e| serde_json::to_value(&e.message).ok())
                .collect(),
        )
    } else {
        None
    };

    Ok(Json(JobStatusResponse {
        job_id: status.job_id,
        video_id: status.video_id,
        status: if is_stale {
            "stale".to_string()
        } else {
            status.status.as_str().to_string()
        },
        progress: status.progress,
        clips_completed: status.clips_completed,
        clips_total: status.clips_total,
        current_step: status.current_step,
        error_message: status.error_message,
        started_at: status.started_at.to_rfc3339(),
        updated_at: status.updated_at.to_rfc3339(),
        last_heartbeat: status.last_heartbeat.map(|h| h.to_rfc3339()),
        is_stale,
        events,
        event_seq: status.event_seq,
    }))
}

/// GET /api/jobs/:job_id/history
///
/// Get full progress history for a job.
///
/// Query parameters:
/// - `since`: Get events since this timestamp (ms)
/// - `limit`: Maximum number of events to return (default: 1000)
///
/// Returns:
/// - 200: Event history
/// - 401: Not authenticated
/// - 403: Job belongs to another user
/// - 404: Job not found
pub async fn get_job_history(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<GetJobHistoryQuery>,
    user: AuthUser,
) -> ApiResult<Json<JobHistoryResponse>> {
    info!(
        "get_job_history uid={} job_id={} since={:?}",
        user.uid, job_id, query.since
    );

    // Validate job ID format
    if !is_valid_job_id(&job_id) {
        return Err(ApiError::bad_request("Invalid job ID format"));
    }

    let job_id_typed = JobId::from(job_id.clone());

    // Get cached status from Redis to verify ownership
    let status = state
        .progress
        .get_job_status(&job_id_typed)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get job status: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Job not found"))?;

    // Verify ownership
    if status.user_id != user.uid {
        return Err(ApiError::forbidden("Access denied"));
    }

    let since = query.since.unwrap_or(0);
    let history = state
        .progress
        .get_history_since(&job_id_typed, since)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?;

    let limit = query.limit.unwrap_or(1000).min(5000) as usize;
    let events: Vec<serde_json::Value> = history
        .into_iter()
        .take(limit)
        .filter_map(|e| {
            let mut value = serde_json::to_value(&e.message).ok()?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("timestamp_ms".to_string(), serde_json::json!(e.timestamp_ms));
                obj.insert("seq".to_string(), serde_json::json!(e.seq));
            }
            Some(value)
        })
        .collect();

    Ok(Json(JobHistoryResponse {
        job_id,
        events,
        event_seq: status.event_seq,
    }))
}

/// Query parameters for job history endpoint.
#[derive(Debug, Deserialize)]
pub struct GetJobHistoryQuery {
    /// Get events since this timestamp (milliseconds since epoch).
    #[serde(default)]
    pub since: Option<i64>,
    /// Maximum number of events to return.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Job history response.
#[derive(Debug, Serialize)]
pub struct JobHistoryResponse {
    pub job_id: String,
    pub events: Vec<serde_json::Value>,
    pub event_seq: u64,
}

// ============================================================================
// Helpers
// ============================================================================

/// Validate job ID format to prevent injection attacks.
///
/// Valid format: alphanumeric characters and hyphens only, 8-64 chars.
fn is_valid_job_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.len() < 8 {
        return false;
    }
    id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_job_ids() {
        assert!(is_valid_job_id("12345678"));
        assert!(is_valid_job_id("abc12345"));
        assert!(is_valid_job_id("abc-1234-def"));
        assert!(is_valid_job_id("a1b2c3d4-e5f6-g7h8"));
    }

    #[test]
    fn test_invalid_job_ids() {
        assert!(!is_valid_job_id(""));
        assert!(!is_valid_job_id("short"));
        assert!(!is_valid_job_id("has space"));
        assert!(!is_valid_job_id("has_underscore"));
        assert!(!is_valid_job_id("has.dot"));
        assert!(!is_valid_job_id(&"a".repeat(65)));
    }
}
