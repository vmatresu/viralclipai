//! Video status and list handlers.
//!
//! Extracted from videos.rs for modularity. Contains:
//! - User videos list (paginated)
//! - Processing status polling endpoint

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use vclip_models::VideoId;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// User videos response.
#[derive(Serialize)]
pub struct UserVideosResponse {
    pub videos: Vec<VideoSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

#[derive(Serialize)]
pub struct VideoSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,
    pub clips_count: u32,
    /// Total size of all clips in bytes.
    pub total_size_bytes: u64,
    /// Human-readable total size.
    pub total_size_formatted: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
}

/// List user videos query params.
#[derive(Deserialize)]
pub struct ListVideosQuery {
    pub limit: Option<u32>,
    pub page_token: Option<String>,
    /// Backward compatible alias for `page_token`.
    pub offset: Option<String>,
}

#[derive(Deserialize)]
pub struct ProcessingStatusQuery {
    pub ids: Option<String>,
}

#[derive(Serialize)]
pub struct ProcessingStatusEntry {
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clips_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct ProcessingStatusResponse {
    pub videos: Vec<ProcessingStatusEntry>,
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;
const MAX_STATUS_IDS: usize = 100;

// ============================================================================
// Handlers
// ============================================================================

/// List user videos with pagination.
pub async fn list_user_videos(
    State(state): State<AppState>,
    Query(query): Query<ListVideosQuery>,
    user: AuthUser,
) -> ApiResult<Json<UserVideosResponse>> {
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );

    let limit = normalize_limit(query.limit);
    let page_token = query
        .page_token
        .as_deref()
        .or(query.offset.as_deref());

    info!(
        "list_user_videos uid={} limit={} has_page_token={}",
        user.uid,
        limit,
        page_token.is_some()
    );

    let (videos, next_page_token) = video_repo.list_page(Some(limit), page_token).await?;

    let summaries: Vec<VideoSummary> = videos
        .into_iter()
        .map(|v| VideoSummary {
            id: v.video_id.as_str().to_string(),
            video_id: Some(v.video_id.as_str().to_string()),
            video_url: Some(v.video_url),
            video_title: Some(v.video_title),
            clips_count: v.clips_count,
            total_size_bytes: v.total_size_bytes,
            total_size_formatted: vclip_models::format_bytes(v.total_size_bytes),
            created_at: Some(v.created_at.to_rfc3339()),
            status: Some(v.status.as_str().to_string()),
            custom_prompt: v.custom_prompt,
        })
        .collect();

    Ok(Json(UserVideosResponse {
        videos: summaries,
        next_page_token,
    }))
}

/// Get processing status for specific video IDs (batch read, no collection scan).
pub async fn get_processing_status(
    State(state): State<AppState>,
    Query(query): Query<ProcessingStatusQuery>,
    user: AuthUser,
) -> ApiResult<Json<ProcessingStatusResponse>> {
    let ids = parse_ids(&query.ids)?;

    validate_ids(&ids)?;

    info!(
        "get_processing_status uid={} ids_count={}",
        user.uid,
        ids.len()
    );

    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );

    let video_ids: Vec<VideoId> = ids.into_iter().map(VideoId::from_string).collect();
    let snapshots = video_repo.get_status_snapshots(&video_ids).await?;

    let videos = snapshots
        .into_iter()
        .map(|s| ProcessingStatusEntry {
            video_id: s.video_id.as_str().to_string(),
            status: s.status.map(|st| st.as_str().to_string()),
            clips_count: s.clips_count,
            updated_at: s.updated_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(ProcessingStatusResponse { videos }))
}

// ============================================================================
// Helpers
// ============================================================================

fn normalize_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(0) | None => DEFAULT_PAGE_SIZE,
        Some(l) if l > MAX_PAGE_SIZE => MAX_PAGE_SIZE,
        Some(l) => l,
    }
}

fn parse_ids(ids_param: &Option<String>) -> ApiResult<Vec<String>> {
    let ids: Vec<String> = ids_param
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if ids.is_empty() {
        return Err(ApiError::bad_request("ids query param is required"));
    }
    if ids.len() > MAX_STATUS_IDS {
        return Err(ApiError::bad_request(format!(
            "Cannot query more than {} ids",
            MAX_STATUS_IDS
        )));
    }

    Ok(ids)
}

fn validate_ids(ids: &[String]) -> ApiResult<()> {
    for id in ids {
        if !is_valid_video_id(id) {
            return Err(ApiError::bad_request("Invalid video ID format"));
        }
    }
    Ok(())
}

/// Validate video ID format to prevent injection attacks.
///
/// Valid format: alphanumeric characters and hyphens only, 8-64 chars.
pub fn is_valid_video_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.len() < 8 {
        return false;
    }
    id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
