//! Storage quota handlers.
//!
//! Provides endpoints for checking and managing storage quotas.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::AppState;

/// Storage quota response.
#[derive(Serialize)]
pub struct StorageQuotaResponse {
    /// Total storage used in bytes.
    pub used_bytes: u64,
    /// Storage limit in bytes.
    pub limit_bytes: u64,
    /// Total number of clips.
    pub total_clips: u32,
    /// Usage percentage (0-100).
    pub percentage: f64,
    /// Human-readable used storage.
    pub used_formatted: String,
    /// Human-readable storage limit.
    pub limit_formatted: String,
    /// Human-readable remaining storage.
    pub remaining_formatted: String,
    /// User's plan.
    pub plan: String,
    /// Whether the user is near the storage limit (>80%).
    pub is_near_limit: bool,
    /// Whether the user has exceeded the storage limit.
    pub is_exceeded: bool,
}

/// Get the current user's storage quota.
pub async fn get_storage_quota(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<StorageQuotaResponse>> {
    let usage = state.user_service.get_storage_usage(&user.uid).await?;
    let limits = state.user_service.get_plan_limits(&user.uid).await?;
    
    let percentage = usage.percentage();
    
    Ok(Json(StorageQuotaResponse {
        used_bytes: usage.total_bytes,
        limit_bytes: usage.limit_bytes,
        total_clips: usage.total_clips,
        percentage,
        used_formatted: usage.format_total(),
        limit_formatted: usage.format_limit(),
        remaining_formatted: usage.format_remaining(),
        plan: limits.plan_id,
        is_near_limit: percentage >= 80.0,
        is_exceeded: percentage >= 100.0,
    }))
}

/// Check if a clip of the given size can be uploaded.
#[derive(serde::Deserialize)]
pub struct CheckQuotaRequest {
    /// Size of the clip to upload in bytes.
    /// Capped at 10 GB to prevent overflow/abuse.
    pub size_bytes: u64,
}

/// Maximum allowed clip size in bytes (10 GB).
/// This prevents abuse and ensures arithmetic stays safe.
const MAX_CLIP_SIZE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Check quota response.
#[derive(Serialize)]
pub struct CheckQuotaResponse {
    /// Whether the upload is allowed.
    pub allowed: bool,
    /// Current usage in bytes.
    pub current_bytes: u64,
    /// Limit in bytes.
    pub limit_bytes: u64,
    /// Requested size in bytes.
    pub requested_bytes: u64,
    /// Human-readable message.
    pub message: String,
}

/// Check if a clip of the given size can be uploaded.
pub async fn check_storage_quota(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CheckQuotaRequest>,
) -> ApiResult<Json<CheckQuotaResponse>> {
    // Clamp the size to prevent abuse and overflow issues
    let size_bytes = request.size_bytes.min(MAX_CLIP_SIZE_BYTES);
    
    // Reject obviously invalid sizes
    if request.size_bytes > MAX_CLIP_SIZE_BYTES {
        return Ok(Json(CheckQuotaResponse {
            allowed: false,
            current_bytes: 0,
            limit_bytes: 0,
            requested_bytes: request.size_bytes,
            message: format!(
                "Requested size {} exceeds maximum allowed clip size of {}",
                vclip_models::format_bytes(request.size_bytes),
                vclip_models::format_bytes(MAX_CLIP_SIZE_BYTES)
            ),
        }));
    }
    
    let usage = state.user_service.get_storage_usage(&user.uid).await?;
    let allowed = !usage.would_exceed(size_bytes);
    
    let message = if allowed {
        format!(
            "Upload allowed. {} remaining after upload.",
            vclip_models::format_bytes(usage.remaining_bytes().saturating_sub(request.size_bytes))
        )
    } else {
        format!(
            "Upload would exceed storage limit. Current: {}, Limit: {}, Requested: {}",
            usage.format_total(),
            usage.format_limit(),
            vclip_models::format_bytes(request.size_bytes)
        )
    };
    
    Ok(Json(CheckQuotaResponse {
        allowed,
        current_bytes: usage.total_bytes,
        limit_bytes: usage.limit_bytes,
        requested_bytes: request.size_bytes,
        message,
    }))
}
