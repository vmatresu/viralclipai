//! Admin handlers for canary testing, monitoring, and user management.

 use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

 use vclip_firestore::{FromFirestoreValue, ToFirestoreValue, Value, FirestoreError};
use vclip_models::{AspectRatio, CropMode, Style};
use vclip_queue::ProcessVideoJob;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
 use crate::security::sanitize_string;
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

    // Parse styles with "all" expansion support
    let style_strs = request.styles.unwrap_or_else(|| vec!["split".to_string()]);
    let styles = Style::expand_styles(&style_strs);

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

#[derive(Serialize)]
pub struct AdminPromptResponse {
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminPromptUpdateRequest {
    pub prompt: String,
}

pub async fn get_admin_prompt(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<AdminPromptResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let doc = state.firestore.get_document("admin", "config").await?;
    let prompt = doc
        .as_ref()
        .and_then(|d| d.fields.as_ref())
        .and_then(|fields| fields.get("base_prompt"))
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    Ok(Json(AdminPromptResponse { prompt }))
}

pub async fn update_admin_prompt(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<AdminPromptUpdateRequest>,
) -> ApiResult<Json<AdminPromptResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let prompt = sanitize_string(&request.prompt);

    let mut fields: HashMap<String, Value> = HashMap::new();
    fields.insert("base_prompt".to_string(), prompt.to_firestore_value());

    let update_mask = Some(vec!["base_prompt".to_string()]);
    match state
        .firestore
        .update_document("admin", "config", fields.clone(), update_mask)
        .await
    {
        Ok(_) => {}
        Err(FirestoreError::NotFound(_)) => {
            state
                .firestore
                .create_document("admin", "config", fields)
                .await?;
        }
        Err(e) => return Err(e.into()),
    }

    info!("Updated global base prompt (admin/config.base_prompt) by admin {}", user.uid);

    Ok(Json(AdminPromptResponse { prompt }))
}

/// User info response for admin views.
#[derive(Serialize)]
pub struct AdminUserResponse {
    pub uid: String,
    pub email: Option<String>,
    pub plan: String,
    pub role: Option<String>,
    /// Credits used this month.
    pub credits_used_this_month: u32,
    /// Monthly credits limit.
    pub monthly_credits_limit: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// List users query params.
#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub page_token: Option<String>,
}

fn default_limit() -> u32 { 20 }

/// List users response.
#[derive(Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<AdminUserResponse>,
    pub next_page_token: Option<String>,
}

/// Get monthly credits limit for a plan (hardcoded fallback for list view performance).
fn get_plan_monthly_credits(plan: &str) -> u32 {
    match plan {
        "free" => vclip_models::plan::FREE_MONTHLY_CREDITS,
        "pro" => vclip_models::plan::PRO_MONTHLY_CREDITS,
        "studio" => vclip_models::plan::STUDIO_MONTHLY_CREDITS,
        _ => vclip_models::plan::FREE_MONTHLY_CREDITS,
    }
}

/// List all users (admin only).
pub async fn list_users(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListUsersQuery>,
) -> ApiResult<Json<ListUsersResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let (users, next_token) = state.user_service
        .list_users(query.limit, query.page_token.as_deref())
        .await?;

    let users: Vec<AdminUserResponse> = users.into_iter().map(|u| {
        let monthly_credits = get_plan_monthly_credits(&u.plan);
        AdminUserResponse {
            uid: u.uid,
            email: u.email,
            plan: u.plan,
            role: u.role,
            credits_used_this_month: u.credits_used_this_month,
            monthly_credits_limit: monthly_credits,
            created_at: u.created_at.to_rfc3339(),
            updated_at: u.updated_at.to_rfc3339(),
        }
    }).collect();

    Ok(Json(ListUsersResponse { users, next_page_token: next_token }))
}

/// Get user by UID (admin only).
pub async fn get_user(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_uid): Path<String>,
) -> ApiResult<Json<AdminUserResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let target = state.user_service
        .get_user_by_uid(&target_uid)
        .await?
        .ok_or_else(|| ApiError::not_found("User not found"))?;

    // Get actual plan limits from Firestore
    let limits = state.user_service.get_plan_limits(&target_uid).await?;

    Ok(Json(AdminUserResponse {
        uid: target.uid,
        email: target.email,
        plan: target.plan,
        role: target.role,
        credits_used_this_month: target.credits_used_this_month,
        monthly_credits_limit: limits.monthly_credits_included,
        created_at: target.created_at.to_rfc3339(),
        updated_at: target.updated_at.to_rfc3339(),
    }))
}

/// Update user plan request.
#[derive(Debug, Deserialize)]
pub struct UpdateUserPlanRequest {
    pub plan: String,
}

/// Update user plan response.
#[derive(Serialize)]
pub struct UpdateUserPlanResponse {
    pub success: bool,
    pub uid: String,
    pub plan: String,
    pub message: String,
}

/// Update user's plan (admin only).
pub async fn update_user_plan(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_uid): Path<String>,
    Json(request): Json<UpdateUserPlanRequest>,
) -> ApiResult<Json<UpdateUserPlanResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    // Validate plan name
    let valid_plans = ["free", "pro", "studio"];
    if !valid_plans.contains(&request.plan.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid plan. Must be one of: {}",
            valid_plans.join(", ")
        )));
    }

    let updated = state.user_service
        .update_user_plan(&target_uid, &request.plan)
        .await?;

    info!("Admin {} updated user {} plan to '{}'", user.uid, target_uid, request.plan);

    Ok(Json(UpdateUserPlanResponse {
        success: true,
        uid: updated.uid,
        plan: updated.plan,
        message: format!("Plan updated to '{}'", request.plan),
    }))
}

/// Update user usage request.
#[derive(Debug, Deserialize)]
pub struct UpdateUserUsageRequest {
    /// Credits to set as used this month.
    pub credits_used: u32,
}

/// Update user usage response.
#[derive(Serialize)]
pub struct UpdateUserUsageResponse {
    pub success: bool,
    pub uid: String,
    pub credits_used_this_month: u32,
    pub message: String,
}

/// Update user's monthly usage (admin only).
pub async fn update_user_usage(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_uid): Path<String>,
    Json(request): Json<UpdateUserUsageRequest>,
) -> ApiResult<Json<UpdateUserUsageResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let updated = state.user_service
        .set_credits(&target_uid, request.credits_used)
        .await?;

    info!("Admin {} set user {} credits usage to {}", user.uid, target_uid, request.credits_used);

    Ok(Json(UpdateUserUsageResponse {
        success: true,
        uid: updated.uid,
        credits_used_this_month: updated.credits_used_this_month,
        message: format!("Credits usage set to {}", request.credits_used),
    }))
}


/// Recalculate storage response.
#[derive(Serialize)]
pub struct RecalculateStorageResponse {
    pub success: bool,
    pub uid: String,
    pub total_storage_bytes: u64,
    pub total_storage_formatted: String,
    pub total_clips: u32,
    pub message: String,
}

/// Recalculate storage for a user (admin only).
/// This is useful for migration or fixing inconsistent storage counts.
pub async fn recalculate_user_storage(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_uid): Path<String>,
) -> ApiResult<Json<RecalculateStorageResponse>> {
    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let (total_bytes, total_clips) = state.user_service
        .recalculate_storage(&target_uid)
        .await?;

    info!(
        "Admin {} recalculated storage for user {}: {} bytes, {} clips",
        user.uid, target_uid, total_bytes, total_clips
    );

    Ok(Json(RecalculateStorageResponse {
        success: true,
        uid: target_uid,
        total_storage_bytes: total_bytes,
        total_storage_formatted: vclip_models::format_bytes(total_bytes),
        total_clips,
        message: format!(
            "Storage recalculated: {} ({} clips)",
            vclip_models::format_bytes(total_bytes),
            total_clips
        ),
    }))
}

/// Reset video status request.
#[derive(Debug, Deserialize)]
pub struct ResetVideoRequest {
    /// New status to set: "completed", "failed", or "analyzed"
    #[serde(default = "default_reset_status")]
    pub status: String,
}

fn default_reset_status() -> String {
    "analyzed".to_string()
}

/// Reset video status response.
#[derive(Serialize)]
pub struct ResetVideoResponse {
    pub success: bool,
    pub video_id: String,
    pub status: String,
    pub message: String,
}

/// Reset a video's status (admin only).
///
/// This is useful for recovering from stuck "processing" states caused by
/// worker crashes (e.g., OOM kills). Sets the video status to allow reprocessing.
pub async fn reset_video_status(
    State(state): State<AppState>,
    user: AuthUser,
    Path((target_uid, video_id)): Path<(String, String)>,
    Json(request): Json<ResetVideoRequest>,
) -> ApiResult<Json<ResetVideoResponse>> {
    use vclip_models::VideoStatus;

    if !state.user_service.is_super_admin(&user.uid).await? {
        return Err(ApiError::forbidden("Admin access required"));
    }

    // Validate status
    let new_status = match request.status.as_str() {
        "completed" => VideoStatus::Completed,
        "failed" => VideoStatus::Failed,
        "analyzed" => VideoStatus::Analyzed,
        other => {
            return Err(ApiError::bad_request(format!(
                "Invalid status '{}'. Must be one of: completed, failed, analyzed",
                other
            )));
        }
    };

    let video_id = vclip_models::VideoId::from(video_id);
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &target_uid,
    );

    // Update video status
    video_repo
        .update_status(&video_id, new_status.clone())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update video status: {}", e)))?;

    info!(
        "Admin {} reset video {} for user {} to status '{}'",
        user.uid, video_id, target_uid, request.status
    );

    Ok(Json(ResetVideoResponse {
        success: true,
        video_id: video_id.to_string(),
        status: request.status,
        message: format!(
            "Video status reset to '{}'. You can now reprocess the video.",
            new_status.as_str()
        ),
    }))
}
