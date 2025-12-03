//! Admin handlers for canary testing and monitoring.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use vclip_models::{AspectRatio, CropMode, Style};
use vclip_queue::ProcessVideoJob;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Synthetic job request for canary testing.
#[derive(Debug, Deserialize)]
pub struct SyntheticJobRequest {
    /// Video URL to process
    pub url: String,
    /// Styles to apply
    #[serde(default)]
    pub styles: Option<Vec<String>>,
    /// Crop mode
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,
    /// Target aspect ratio
    #[serde(default = "default_aspect")]
    pub target_aspect: String,
    /// Custom prompt
    #[serde(default)]
    pub prompt: Option<String>,
}

fn default_crop_mode() -> String {
    "none".to_string()
}

fn default_aspect() -> String {
    "9:16".to_string()
}

/// Synthetic job response.
#[derive(Serialize)]
pub struct SyntheticJobResponse {
    pub success: bool,
    pub job_id: String,
    pub video_id: String,
    pub message: String,
}

/// Enqueue a synthetic job for canary testing.
/// Only accessible to superadmins.
pub async fn enqueue_synthetic_job(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<SyntheticJobRequest>,
) -> ApiResult<Json<SyntheticJobResponse>> {
    // Check if user is superadmin
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    // Parse styles
    let styles: Vec<Style> = request
        .styles
        .unwrap_or_else(|| vec!["split".to_string()])
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if styles.is_empty() {
        return Err(ApiError::bad_request("No valid styles specified"));
    }

    // Parse crop mode and target aspect
    let crop_mode: CropMode = request.crop_mode.parse().unwrap_or_default();
    let target_aspect: AspectRatio = request.target_aspect.parse().unwrap_or_default();

    // Create job
    let job = ProcessVideoJob::new(&user.uid, &request.url, styles)
        .with_crop_mode(crop_mode)
        .with_target_aspect(target_aspect)
        .with_custom_prompt(request.prompt);

    let job_id = job.job_id.to_string();
    let video_id = job.video_id.to_string();

    // Enqueue job
    state.queue.enqueue_process(job).await?;

    info!(
        "Admin {} enqueued synthetic job {} for video {}",
        user.uid, job_id, video_id
    );

    Ok(Json(SyntheticJobResponse {
        success: true,
        job_id,
        video_id: video_id.clone(),
        message: format!("Synthetic job enqueued. Video ID: {}", video_id),
    }))
}

/// Queue status response.
#[derive(Serialize)]
pub struct QueueStatusResponse {
    pub queue_length: u64,
    pub dlq_length: u64,
}

/// Get queue status (admin only).
pub async fn get_queue_status(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<QueueStatusResponse>> {
    // Check if user is superadmin
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let queue_length = state.queue.len().await.unwrap_or(0);
    let dlq_length = state.queue.dlq_len().await.unwrap_or(0);

    Ok(Json(QueueStatusResponse {
        queue_length,
        dlq_length,
    }))
}

/// System info response.
#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub version: String,
    pub rust_version: String,
    pub build_timestamp: String,
}

/// Get system info (admin only).
pub async fn get_system_info(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<SystemInfoResponse>> {
    // Check if user is superadmin
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    Ok(Json(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        rust_version: env!("CARGO_PKG_RUST_VERSION").to_string(),
        build_timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
