//! Video API handlers.

use std::collections::HashMap;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use vclip_models::VideoId;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Video info response.
#[derive(Serialize)]
pub struct VideoInfoResponse {
    pub id: String,
    pub clips: Vec<ClipInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
}

#[derive(Serialize)]
pub struct ClipInfo {
    pub name: String,
    pub title: String,
    pub description: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// Get video info.
pub async fn get_video_info(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<VideoInfoResponse>> {
    let video_id_obj = VideoId::from_string(&video_id);

    // Load highlights from R2
    let highlights = state
        .storage
        .load_highlights(&user.uid, &video_id)
        .await
        .map_err(|e| {
            if matches!(e, vclip_storage::StorageError::NotFound(_)) {
                ApiError::not_found("Video not found")
            } else {
                ApiError::Storage(e)
            }
        })?;

    // Build highlights map for metadata extraction
    let highlights_map: HashMap<u32, (String, String)> = highlights
        .highlights
        .iter()
        .map(|h| (h.id, (h.title.clone(), h.description.clone().unwrap_or_default())))
        .collect();

    // List clips from R2
    let clips = state
        .storage
        .list_clips_with_metadata(&user.uid, &video_id, &highlights_map, Duration::from_secs(3600))
        .await?;

    // Convert to response format
    let clips_response: Vec<ClipInfo> = clips
        .into_iter()
        .map(|c| ClipInfo {
            name: c.name,
            title: c.title,
            description: c.description,
            url: c.url,
            direct_url: c.direct_url,
            thumbnail: c.thumbnail,
            size: c.size,
            style: c.style,
        })
        .collect();

    Ok(Json(VideoInfoResponse {
        id: video_id,
        clips: clips_response,
        custom_prompt: highlights.custom_prompt,
        video_title: highlights.video_title,
        video_url: highlights.video_url,
    }))
}

/// User videos response.
#[derive(Serialize)]
pub struct UserVideosResponse {
    pub videos: Vec<VideoSummary>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// List user videos query params.
#[derive(Deserialize)]
pub struct ListVideosQuery {
    pub limit: Option<u32>,
    pub offset: Option<String>,
}

/// List user videos.
pub async fn list_user_videos(
    State(state): State<AppState>,
    Query(query): Query<ListVideosQuery>,
    user: AuthUser,
) -> ApiResult<Json<UserVideosResponse>> {
    // Create video repository
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );

    // List videos
    let videos = video_repo.list(query.limit).await?;

    // Convert to response format
    let summaries: Vec<VideoSummary> = videos
        .into_iter()
        .map(|v| VideoSummary {
            id: v.video_id.as_str().to_string(),
            video_id: Some(v.video_id.as_str().to_string()),
            video_url: Some(v.video_url),
            video_title: Some(v.video_title),
            clips_count: v.clips_count,
            created_at: Some(v.created_at.to_rfc3339()),
        })
        .collect();

    Ok(Json(UserVideosResponse { videos: summaries }))
}

/// Delete video response.
#[derive(Serialize)]
pub struct DeleteVideoResponse {
    pub success: bool,
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_deleted: Option<u32>,
}

/// Delete a video.
pub async fn delete_video(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<DeleteVideoResponse>> {
    // Delete files from R2
    let files_deleted = state
        .storage
        .delete_video_files(&user.uid, &video_id)
        .await?;

    // Delete from Firestore
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    video_repo.delete(&VideoId::from_string(&video_id)).await?;

    Ok(Json(DeleteVideoResponse {
        success: true,
        video_id,
        message: Some("Video deleted successfully".to_string()),
        files_deleted: Some(files_deleted),
    }))
}
