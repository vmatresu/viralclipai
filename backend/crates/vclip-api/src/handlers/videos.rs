//! Video API handlers.

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

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
    /// Unique clip ID from Firestore (primary identifier)
    pub clip_id: String,
    /// Filename (for backward compatibility and legacy endpoints)
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
    /// When the clip was completed (for cache busting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// When the clip was last updated (for cache busting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

// ============================================================================
// Scene/Style Index (for overwrite confirmation)
// ============================================================================

#[derive(Serialize)]
pub struct SceneStyleEntry {
    pub scene_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_title: Option<String>,
    pub styles: Vec<String>,
}

#[derive(Serialize)]
pub struct SceneStyleResponse {
    pub video_id: String,
    pub scene_styles: Vec<SceneStyleEntry>,
}

/// Return the list of scene/style combinations already generated for a video.
/// This lets the frontend present an accurate overwrite confirmation dialog.
pub async fn get_video_scene_styles(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<SceneStyleResponse>> {
    // Validate video_id format
    if !is_valid_video_id(&video_id) {
        return Err(ApiError::bad_request("Invalid video ID format"));
    }

    // Verify ownership early
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);

    // Best-effort highlight titles from Firestore (do not fail if missing)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    
    let highlight_titles: HashMap<u32, String> = highlights_repo
        .get(&video_id_obj)
        .await
        .ok()
        .and_then(|opt| opt)
        .map(|vh| {
            vh.highlights
                .into_iter()
                .map(|h| (h.id, h.title))
                .collect()
        })
        .unwrap_or_default();

    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        video_id_obj.clone(),
    );
    let clips = clip_repo.list(None).await.map_err(ApiError::from)?;

    let mut index: HashMap<u32, SceneStyleEntry> = HashMap::new();
    for clip in clips {
        let entry = index.entry(clip.scene_id).or_insert_with(|| SceneStyleEntry {
            scene_id: clip.scene_id,
            scene_title: highlight_titles
                .get(&clip.scene_id)
                .cloned()
                .or_else(|| Some(clip.scene_title.clone())),
            styles: Vec::new(),
        });

        if !entry
            .styles
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&clip.style))
        {
            entry.styles.push(clip.style.clone());
        }
    }

    let mut scene_styles: Vec<SceneStyleEntry> = index.into_values().collect();
    scene_styles.sort_by_key(|e| e.scene_id);
    for entry in &mut scene_styles {
        entry.styles.sort();
    }

    Ok(Json(SceneStyleResponse {
        video_id,
        scene_styles,
    }))
}

/// Get video info.
pub async fn get_video_info(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<VideoInfoResponse>> {
    // Validate video_id format (UUID-like, alphanumeric + hyphens only)
    if !is_valid_video_id(&video_id) {
        return Err(ApiError::bad_request("Invalid video ID format"));
    }

    let video_id_obj = VideoId::from_string(&video_id);

    // Verify ownership first
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Get video metadata from Firestore to check status
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    
    let video_meta = video_repo.get(&video_id_obj).await
        .map_err(|e| {
            warn!("Failed to query video metadata for {}: {}", video_id, e);
            ApiError::internal(format!("Database error: {}", e))
        })?
        .ok_or_else(|| ApiError::not_found("Video not found"))?;

    // Load highlights from Firestore (source of truth)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );

    let highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| {
            warn!("Highlights not found for video {}, status: {:?}", video_id, video_meta.status);
            ApiError::Conflict(
                "Video is still being processed. Highlights will be available once AI analysis completes.".to_string()
            )
        })?
        .to_highlights_data();

    // Build highlights map for metadata extraction
    let highlights_map: HashMap<u32, (String, String)> = highlights
        .highlights
        .iter()
        .map(|h| (h.id, (h.title.clone(), h.description.clone().unwrap_or_default())))
        .collect();

    // Get ALL clips from Firestore (primary source of truth for metadata)
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        video_id_obj.clone(),
    );
    let firestore_clips = clip_repo.list(None).await.unwrap_or_default();

    // Convert Firestore clips to API format (async URL generation)
    let mut clips: Vec<ClipInfo> = Vec::new();
    for clip_meta in firestore_clips {
        // Find matching highlight for title/description
        let (title, description) = highlights_map
            .get(&clip_meta.scene_id)
            .cloned()
            .unwrap_or_else(|| (clip_meta.scene_title.clone(), "Generated clip".to_string()));

        // Generate URLs using the r2_key stored in Firestore metadata
        let api_url = format!("/api/videos/{}/clips/{}", video_id, clip_meta.filename);
        let direct_url = state.storage.get_url(&clip_meta.r2_key, Duration::from_secs(3600)).await.ok();
        let thumbnail_url = if let Some(ref key) = clip_meta.thumbnail_r2_key {
            state.storage.get_url(key, Duration::from_secs(3600)).await.ok()
        } else {
            None
        };

        // Format file size
        let size_mb = clip_meta.file_size_bytes as f64 / (1024.0 * 1024.0);
        let size_str = format!("{:.1} MB", size_mb);

        clips.push(ClipInfo {
            clip_id: clip_meta.clip_id.clone(),
            name: clip_meta.filename,
            title,
            description,
            url: api_url,
            direct_url,
            thumbnail: thumbnail_url,
            size: size_str,
            style: Some(clip_meta.style),
            completed_at: clip_meta.completed_at.map(|dt| dt.to_rfc3339()),
            updated_at: clip_meta.updated_at.map(|dt| dt.to_rfc3339()),
        });
    }

    // Sort by name for consistent display
    clips.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(VideoInfoResponse {
        id: video_id,
        clips,
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
            total_size_bytes: v.total_size_bytes,
            total_size_formatted: vclip_models::format_bytes(v.total_size_bytes),
            created_at: Some(v.created_at.to_rfc3339()),
            status: Some(v.status.as_str().to_string()),
            custom_prompt: v.custom_prompt,
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
    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Get video metadata to know the total size for storage tracking
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    let video_size = video_repo.get(&VideoId::from_string(&video_id)).await
        .ok()
        .flatten()
        .map(|v| v.total_size_bytes)
        .unwrap_or(0);

    // Get clip count for storage tracking
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        VideoId::from_string(&video_id),
    );
    let clip_count = clip_repo.list(None).await.map(|c| c.len() as u32).unwrap_or(0);

    // Delete files from R2
    let files_deleted = state
        .storage
        .delete_video_files(&user.uid, &video_id)
        .await?;

    // Delete from Firestore
    video_repo.delete(&VideoId::from_string(&video_id)).await?;

    // Update user's total storage - recalculate to ensure consistency
    if video_size > 0 || clip_count > 0 {
        // Recalculate storage to ensure consistency after video deletion
        if let Err(e) = state.user_service.recalculate_storage(&user.uid).await {
            warn!("Failed to recalculate storage after deleting video {}: {}", video_id, e);
        }
    }

    info!("Deleted video {} for user {} ({} files, {} bytes)", video_id, user.uid, files_deleted, video_size);

    Ok(Json(DeleteVideoResponse {
        success: true,
        video_id,
        message: Some("Video deleted successfully".to_string()),
        files_deleted: Some(files_deleted),
    }))
}

// ============================================================================
// Bulk Delete Videos
// ============================================================================

/// Bulk delete videos request.
#[derive(Debug, Deserialize)]
pub struct BulkDeleteVideosRequest {
    pub video_ids: Vec<String>,
}

/// Bulk delete videos response.
#[derive(Serialize)]
pub struct BulkDeleteVideosResponse {
    pub success: bool,
    pub deleted_count: u32,
    pub failed_count: u32,
    pub results: HashMap<String, BulkDeleteResult>,
}

#[derive(Serialize)]
pub struct BulkDeleteResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_deleted: Option<u32>,
}

/// Bulk delete videos.
pub async fn bulk_delete_videos(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<BulkDeleteVideosRequest>,
) -> ApiResult<Json<BulkDeleteVideosResponse>> {
    if request.video_ids.is_empty() {
        return Err(ApiError::bad_request("At least one video ID is required"));
    }

    if request.video_ids.len() > 100 {
        return Err(ApiError::bad_request("Cannot delete more than 100 videos at once"));
    }

    let mut results = HashMap::new();
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;

    for video_id in &request.video_ids {
        // Check ownership
        let is_owner = state.user_service.user_owns_video(&user.uid, video_id).await.unwrap_or(false);
        
        if !is_owner {
            results.insert(video_id.clone(), BulkDeleteResult {
                success: false,
                error: Some("Video not found or access denied".to_string()),
                files_deleted: None,
            });
            failed_count += 1;
            continue;
        }

        // Delete files from R2
        let files_deleted = match state.storage.delete_video_files(&user.uid, video_id).await {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to delete files for video {}: {}", video_id, e);
                0
            }
        };

        // Delete from Firestore
        let video_repo = vclip_firestore::VideoRepository::new(
            (*state.firestore).clone(),
            &user.uid,
        );
        
        match video_repo.delete(&VideoId::from_string(video_id)).await {
            Ok(_) => {
                results.insert(video_id.clone(), BulkDeleteResult {
                    success: true,
                    error: None,
                    files_deleted: Some(files_deleted),
                });
                deleted_count += 1;
                info!("Deleted video {} for user {} ({} files)", video_id, user.uid, files_deleted);
            }
            Err(e) => {
                results.insert(video_id.clone(), BulkDeleteResult {
                    success: false,
                    error: Some(format!("Database error: {}", e)),
                    files_deleted: Some(files_deleted),
                });
                failed_count += 1;
            }
        }
    }

    Ok(Json(BulkDeleteVideosResponse {
        success: deleted_count > 0,
        deleted_count,
        failed_count,
        results,
    }))
}

// ============================================================================
// Clip Streaming
// ============================================================================

/// Stream a video clip with range request support.
pub async fn stream_clip(
    State(state): State<AppState>,
    Path((video_id, clip_name)): Path<(String, String)>,
    headers: HeaderMap,
    user: AuthUser,
) -> Result<Response, ApiError> {
    // Validate clip name
    if clip_name.contains("..") || clip_name.contains('/') || clip_name.contains('\\') {
        return Err(ApiError::bad_request("Invalid clip name"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    let key = format!("{}/{}/clips/{}", user.uid, video_id, clip_name);

    // Determine content type
    let content_type = if clip_name.to_lowercase().ends_with(".mp4") {
        "video/mp4"
    } else if clip_name.to_lowercase().ends_with(".jpg") || clip_name.to_lowercase().ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };

    // Handle range requests
    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (bytes, content_length, _) = state
        .storage
        .get_object_range(&key, range_header.as_deref())
        .await
        .map_err(|e| {
            if matches!(e, vclip_storage::StorageError::NotFound(_)) {
                ApiError::not_found("Clip not found")
            } else {
                ApiError::Storage(e)
            }
        })?;

    // Build response
    let mut response_builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .header("Cross-Origin-Resource-Policy", "cross-origin");

    if range_header.is_some() {
        response_builder = response_builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_LENGTH, bytes.len());
    } else {
        response_builder = response_builder
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, content_length);
    }

    response_builder
        .body(Body::from(bytes))
        .map_err(|e| ApiError::internal(format!("Failed to build response: {}", e)))
}

// ============================================================================
// Delete Clip
// ============================================================================

/// Delete clip response.
#[derive(Serialize)]
pub struct DeleteClipResponse {
    pub success: bool,
    pub video_id: String,
    pub clip_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_deleted: Option<u32>,
}

/// Delete a single clip.
pub async fn delete_clip(
    State(state): State<AppState>,
    Path((video_id, clip_name)): Path<(String, String)>,
    user: AuthUser,
) -> ApiResult<Json<DeleteClipResponse>> {
    // Validate clip name
    if clip_name.contains("..") || clip_name.contains('/') || clip_name.contains('\\') {
        return Err(ApiError::bad_request("Invalid clip name"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Get clip metadata first to know the file size for storage tracking
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        vclip_models::VideoId::from_string(&video_id),
    );

    // Prefer Firestore metadata, but fall back to R2 object size if Firestore lookup fails.
    let mut clip_size_bytes: Option<u64> = match clip_repo.list(None).await {
        Ok(clips) => clips
            .iter()
            .find(|c| c.filename == clip_name)
            .map(|c| c.file_size_bytes),
        Err(e) => {
            warn!(
                "Failed to list clips for size lookup ({} / {}): {}. Falling back to storage.",
                user.uid, video_id, e
            );
            None
        }
    };

    if clip_size_bytes.is_none() {
        let object_prefix = format!("{}/{}/clips/{}", user.uid, video_id, clip_name);
        match state.storage.list_objects(&object_prefix).await {
            Ok(objects) => {
                clip_size_bytes = objects
                    .into_iter()
                    .find(|o| o.key.ends_with(&clip_name))
                    .map(|o| o.size);
            }
            Err(e) => {
                warn!(
                    "Failed to read clip size from storage for {}/{}/{}: {}",
                    user.uid, video_id, clip_name, e
                );
            }
        }
    }

    let clip_size = clip_size_bytes.unwrap_or(0);
    let size_unknown = clip_size == 0;
    if size_unknown {
        warn!(
            "Clip size unknown for {}/{}/{}; will trigger storage recalculation",
            user.uid, video_id, clip_name
        );
    }

    // Delete clip and thumbnail from R2
    let files_deleted = state
        .storage
        .delete_clip(&user.uid, &video_id, &clip_name)
        .await?;

    // Delete clip metadata from Firestore
    let metadata_deleted = clip_repo.delete_by_filename(&clip_name).await?;

    // Update storage counters
    if size_unknown {
        // When size is unknown, recalculate to ensure consistency.
        // This is slower but ensures we don't drift out of sync.
        if let Err(e) = state.user_service.recalculate_storage(&user.uid).await {
            warn!(
                "Failed to recalculate storage after deleting clip with unknown size {}: {}",
                clip_name, e
            );
        }
    } else {
        // Update video's total size
        let video_repo = vclip_firestore::VideoRepository::new((*state.firestore).clone(), &user.uid);
        if let Err(e) = video_repo
            .subtract_clip_size(&vclip_models::VideoId::from_string(&video_id), clip_size)
            .await
        {
            warn!(
                "Failed to update video total size after deleting {}: {}",
                clip_name, e
            );
        }

        // Decrement user's storage
        if let Err(e) = state
            .user_service
            .subtract_storage(&user.uid, clip_size)
            .await
        {
            warn!(
                "Failed to update user storage after deleting {}: {}",
                clip_name, e
            );
        }
    }

    // Refresh video clips_count to reflect deletion
    if let Err(e) = refresh_clips_count(&state, &user.uid, &video_id).await {
        warn!("Failed to refresh clips_count after deleting {}: {}", clip_name, e);
    }

    info!("Deleted clip {} from video {} for user {} ({} files, {} bytes, metadata deleted: {})", 
          clip_name, video_id, user.uid, files_deleted, clip_size, metadata_deleted);

    Ok(Json(DeleteClipResponse {
        success: true,
        video_id,
        clip_name,
        message: Some(if files_deleted > 0 {
            "Clip deleted successfully".to_string()
        } else {
            "Clip already deleted".to_string()
        }),
        files_deleted: Some(files_deleted),
    }))
}

// ============================================================================
// Bulk Delete Clips
// ============================================================================

/// Bulk delete clips request.
#[derive(Debug, Deserialize)]
pub struct BulkDeleteClipsRequest {
    pub clip_names: Vec<String>,
}

/// Bulk delete clips response.
#[derive(Serialize)]
pub struct BulkDeleteClipsResponse {
    pub success: bool,
    pub video_id: String,
    pub deleted_count: u32,
    pub failed_count: u32,
    pub results: HashMap<String, BulkDeleteResult>,
}

/// Delete all clips for a video response.
#[derive(Serialize)]
pub struct DeleteAllClipsResponse {
    pub success: bool,
    pub video_id: String,
    pub deleted_count: u32,
    pub failed_count: u32,
    pub results: HashMap<String, BulkDeleteResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Recompute clips_count for a video and persist to Firestore.
async fn refresh_clips_count(state: &AppState, user_id: &str, video_id: &str) -> ApiResult<()> {
    let video_id_obj = vclip_models::VideoId::from_string(video_id);
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        user_id,
        video_id_obj.clone(),
    );

    let video_repo = vclip_firestore::VideoRepository::new((*state.firestore).clone(), user_id);

    let clips = clip_repo.list(None).await.map_err(ApiError::from)?;
    let new_count = clips.len() as u32;

    video_repo
        .update_clips_count(&video_id_obj, new_count)
        .await
        .map_err(ApiError::from)?;

    Ok(())
}

/// Bulk delete clips for a video.
pub async fn bulk_delete_clips(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<BulkDeleteClipsRequest>,
) -> ApiResult<Json<BulkDeleteClipsResponse>> {
    if request.clip_names.is_empty() {
        return Err(ApiError::bad_request("At least one clip name is required"));
    }

    if request.clip_names.len() > 100 {
        return Err(ApiError::bad_request("Cannot delete more than 100 clips at once"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Validate clip names
    for clip_name in &request.clip_names {
        if clip_name.contains("..") || clip_name.contains('/') || clip_name.contains('\\') {
            return Err(ApiError::bad_request(format!("Invalid clip name: {}", clip_name)));
        }
    }

    let mut results = HashMap::new();
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;
    let mut any_unknown_sizes = false;

    let video_id_obj = vclip_models::VideoId::from_string(&video_id);
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        video_id_obj.clone(),
    );
    let video_repo = vclip_firestore::VideoRepository::new((*state.firestore).clone(), &user.uid);

    // Build a lookup of clip sizes from Firestore so we can subtract accurate storage.
    let clip_sizes: HashMap<String, u64> = clip_repo
        .list(None)
        .await
        .map(|clips| {
            clips
                .into_iter()
                .map(|c| (c.filename, c.file_size_bytes))
                .collect()
        })
        .unwrap_or_default();

    for clip_name in &request.clip_names {
        // Delete clip and thumbnail from R2
        let files_deleted = match state.storage.delete_clip(&user.uid, &video_id, clip_name).await {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to delete files for clip {}: {}", clip_name, e);
                results.insert(clip_name.clone(), BulkDeleteResult {
                    success: false,
                    error: Some(format!("Storage error: {}", e)),
                    files_deleted: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // Delete clip metadata from Firestore
        let metadata_deleted = match clip_repo.delete_by_filename(clip_name).await {
            Ok(deleted) => deleted,
            Err(e) => {
                warn!("Failed to delete metadata for clip {}: {}", clip_name, e);
                results.insert(clip_name.clone(), BulkDeleteResult {
                    success: false,
                    error: Some(format!("Database error: {}", e)),
                    files_deleted: Some(files_deleted),
                });
                failed_count += 1;
                continue;
            }
        };

        // Determine clip size (prefer Firestore metadata, fallback to storage listing)
        let mut clip_size = clip_sizes.get(clip_name).copied().unwrap_or(0);
        if clip_size == 0 {
            let object_prefix = format!("{}/{}/clips/{}", user.uid, video_id, clip_name);
            if let Ok(objects) = state.storage.list_objects(&object_prefix).await {
                clip_size = objects
                    .into_iter()
                    .find(|o| o.key.ends_with(clip_name))
                    .map(|o| o.size)
                    .unwrap_or(0);
            }
        }

        let size_was_known = clip_size > 0;
        if size_was_known {
            if let Err(e) = video_repo
                .subtract_clip_size(&video_id_obj, clip_size)
                .await
            {
                warn!(
                    "Failed to update video total size after deleting {}: {}",
                    clip_name, e
                );
            }

            if let Err(e) = state
                .user_service
                .subtract_storage(&user.uid, clip_size)
                .await
            {
                warn!(
                    "Failed to update user storage after deleting {}: {}",
                    clip_name, e
                );
            }
        } else {
            // Track that we had an unknown size - we'll recalculate at the end
            any_unknown_sizes = true;
        }

        results.insert(clip_name.clone(), BulkDeleteResult {
            success: true,
            error: None,
            files_deleted: Some(files_deleted),
        });
        deleted_count += 1;

        info!("Deleted clip {} from video {} for user {} ({} files, metadata deleted: {})", 
              clip_name, video_id, user.uid, files_deleted, metadata_deleted);
    }

    // If any clips had unknown sizes, recalculate storage to ensure consistency
    if any_unknown_sizes {
        info!(
            "Some clips had unknown sizes during bulk delete for {}/{}; recalculating storage",
            user.uid, video_id
        );
        if let Err(e) = state.user_service.recalculate_storage(&user.uid).await {
            warn!("Failed to recalculate storage after bulk delete: {}", e);
        }
    }

    // Refresh video clips_count to reflect deletions
    if let Err(e) = refresh_clips_count(&state, &user.uid, &video_id).await {
        warn!("Failed to refresh clips_count after bulk delete: {}", e);
    }

    Ok(Json(BulkDeleteClipsResponse {
        success: deleted_count > 0,
        video_id,
        deleted_count,
        failed_count,
        results,
    }))
}

/// Delete all clips for a video.
pub async fn delete_all_clips(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<DeleteAllClipsResponse>> {
    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Get all clips for this video from Firestore
    let video_id_obj = vclip_models::VideoId::from_string(&video_id);
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        video_id_obj.clone(),
    );
    let video_repo = vclip_firestore::VideoRepository::new((*state.firestore).clone(), &user.uid);
    
    let clips = match clip_repo.list(None).await {
        Ok(clips) => clips,
        Err(e) => {
            warn!("Failed to list clips for video {}: {}", video_id, e);
            return Err(ApiError::internal(format!("Failed to retrieve clips: {}", e)));
        }
    };

    if clips.is_empty() {
        return Ok(Json(DeleteAllClipsResponse {
            success: true,
            video_id,
            deleted_count: 0,
            failed_count: 0,
            results: HashMap::new(),
            message: Some("No clips found to delete".to_string()),
        }));
    }

    let clip_names: Vec<String> = clips.iter().map(|c| c.filename.clone()).collect();

    let mut results = HashMap::new();
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;

    for clip in clips {
        let clip_name = clip.filename;

        // Delete clip and thumbnail from R2
        let files_deleted = match state.storage.delete_clip(&user.uid, &video_id, &clip_name).await {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to delete files for clip {}: {}", clip_name, e);
                results.insert(clip_name.clone(), BulkDeleteResult {
                    success: false,
                    error: Some(format!("Storage error: {}", e)),
                    files_deleted: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // Delete clip metadata from Firestore
        match clip_repo.delete_by_filename(&clip_name).await {
            Ok(deleted) => {
                if deleted {
                    results.insert(clip_name.clone(), BulkDeleteResult {
                        success: true,
                        error: None,
                        files_deleted: Some(files_deleted),
                    });
                    deleted_count += 1;
                    info!("Deleted clip {} from video {} for user {} ({} files)", 
                          clip_name, video_id, user.uid, files_deleted);

                    let clip_size = clip.file_size_bytes;
                    if clip_size > 0 {
                        if let Err(e) = video_repo
                            .subtract_clip_size(&video_id_obj, clip_size)
                            .await
                        {
                            warn!(
                                "Failed to update video total size after deleting {}: {}",
                                clip_name, e
                            );
                        }
                    }

                    if let Err(e) = state
                        .user_service
                        .subtract_storage(&user.uid, clip_size)
                        .await
                    {
                        warn!(
                            "Failed to update user storage after deleting {}: {}",
                            clip_name, e
                        );
                    }
                } else {
                    // Clip metadata not found
                    results.insert(clip_name.clone(), BulkDeleteResult {
                        success: false,
                        error: Some("Clip metadata not found".to_string()),
                        files_deleted: Some(files_deleted),
                    });
                    failed_count += 1;
                }
            }
            Err(e) => {
                warn!("Failed to delete metadata for clip {}: {}", clip_name, e);
                results.insert(clip_name.clone(), BulkDeleteResult {
                    success: false,
                    error: Some(format!("Database error: {}", e)),
                    files_deleted: Some(files_deleted),
                });
                failed_count += 1;
            }
        }
    }

    // Refresh video clips_count to reflect deletions
    if let Err(e) = refresh_clips_count(&state, &user.uid, &video_id).await {
        warn!("Failed to refresh clips_count after delete all: {}", e);
    }

    Ok(Json(DeleteAllClipsResponse {
        success: deleted_count > 0,
        video_id,
        deleted_count,
        failed_count,
        results,
        message: Some(format!("Deleted {} out of {} clips", deleted_count, clip_names.len())),
    }))
}

// ============================================================================
// Highlights
// ============================================================================

/// Highlight info.
#[derive(Serialize)]
pub struct HighlightInfo {
    pub id: u32,
    pub title: String,
    pub start: String,
    pub end: String,
    pub duration: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Highlights response.
#[derive(Serialize)]
pub struct HighlightsResponse {
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,
    pub highlights: Vec<HighlightInfo>,
}

/// Get video highlights.
pub async fn get_video_highlights(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<HighlightsResponse>> {
    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);

    // Load highlights from Firestore (source of truth)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );

    let video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    let highlights: Vec<HighlightInfo> = video_highlights
        .highlights
        .into_iter()
        .map(|h| HighlightInfo {
            id: h.id,
            title: h.title,
            start: h.start,
            end: h.end,
            duration: h.duration,
            hook_category: h.hook_category.map(|c| format!("{:?}", c).to_lowercase()),
            reason: h.reason,
            description: h.description,
        })
        .collect();

    Ok(Json(HighlightsResponse {
        video_id,
        video_url: video_highlights.video_url,
        video_title: video_highlights.video_title,
        highlights,
    }))
}

// ============================================================================
// Update Video Title
// ============================================================================

/// Update video title request.
#[derive(Debug, Deserialize)]
pub struct UpdateVideoTitleRequest {
    pub title: String,
}

/// Update video title response.
#[derive(Serialize)]
pub struct UpdateVideoTitleResponse {
    pub success: bool,
    pub video_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Update video title.
pub async fn update_video_title(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<UpdateVideoTitleRequest>,
) -> ApiResult<Json<UpdateVideoTitleResponse>> {
    // Validate title
    let title = request.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }
    if title.len() > 500 {
        return Err(ApiError::bad_request("Title too long (max 500 characters)"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Update title
    let updated = state
        .user_service
        .update_video_title(&user.uid, &video_id, title)
        .await?;

    if !updated {
        return Err(ApiError::not_found("Video not found"));
    }

    info!("Updated title for video {} for user {}", video_id, user.uid);

    Ok(Json(UpdateVideoTitleResponse {
        success: true,
        video_id,
        title: title.to_string(),
        message: Some("Title updated successfully".to_string()),
    }))
}

// ============================================================================
// Reprocess Scenes (POST endpoint)
// ============================================================================

/// Reprocess scenes request.
#[derive(Debug, Deserialize)]
pub struct ReprocessScenesRequest {
    pub scene_ids: Vec<u32>,
    pub styles: Vec<String>,
}

/// Reprocess scenes response.
#[derive(Serialize)]
pub struct ReprocessScenesResponse {
    pub success: bool,
    pub video_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

/// Initiate scene reprocessing (POST endpoint).
pub async fn reprocess_scenes(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<ReprocessScenesRequest>,
) -> ApiResult<Json<ReprocessScenesResponse>> {
    // Validate request
    if request.scene_ids.is_empty() {
        return Err(ApiError::bad_request("At least one scene ID is required"));
    }
    if request.scene_ids.len() > 50 {
        return Err(ApiError::bad_request("Cannot reprocess more than 50 scenes at once"));
    }
    if request.styles.is_empty() {
        return Err(ApiError::bad_request("At least one style is required"));
    }
    if request.styles.len() > 10 {
        return Err(ApiError::bad_request("Cannot use more than 10 styles"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Check if video is currently processing
    if state.user_service.is_video_processing(&user.uid, &video_id).await? {
        return Err(ApiError::Conflict(
            "Video is currently processing. Please wait for it to complete before reprocessing.".to_string()
        ));
    }

    // Check plan restrictions
    if !state.user_service.has_pro_or_studio_plan(&user.uid).await? {
        return Err(ApiError::forbidden(
            "Scene reprocessing is only available for Pro and Studio plans. Please upgrade to access this feature."
        ));
    }

    // Check if user has exceeded monthly clip quota
    let used = state.user_service.get_monthly_usage(&user.uid).await?;
    let limits = state.user_service.get_plan_limits(&user.uid).await?;
    if used >= limits.max_clips_per_month {
        return Err(ApiError::forbidden(format!(
            "Monthly clip limit exceeded. You've used {} of {} clips this month. Please upgrade your plan or wait until next month.",
            used, limits.max_clips_per_month
        )));
    }

    // Check storage quota
    let storage_usage = state.user_service.get_storage_usage(&user.uid).await?;
    if storage_usage.percentage() >= 100.0 {
        return Err(ApiError::forbidden(format!(
            "Storage limit exceeded. You've used {} of {} storage. Please delete some clips or upgrade your plan.",
            storage_usage.format_total(), storage_usage.format_limit()
        )));
    }

    // Validate plan limits for the number of clips being created
    let total_clips = request.scene_ids.len() as u32 * request.styles.len() as u32;
    state.user_service.validate_plan_limits(&user.uid, total_clips).await?;

    // Load highlights from Firestore to validate scene IDs
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    
    let video_highlights = highlights_repo
        .get(&VideoId::from_string(&video_id))
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    let available_ids: std::collections::HashSet<u32> = video_highlights
        .highlights
        .iter()
        .map(|h| h.id)
        .collect();

    let invalid_ids: Vec<u32> = request
        .scene_ids
        .iter()
        .filter(|id| !available_ids.contains(id))
        .copied()
        .collect();

    if !invalid_ids.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Invalid scene IDs: {:?}. Available: {:?}",
            invalid_ids,
            available_ids.iter().collect::<Vec<_>>()
        )));
    }

    // Parse styles with "all" expansion support
    let styles = vclip_models::Style::expand_styles(&request.styles);
    
    if styles.is_empty() {
        return Err(ApiError::bad_request("No valid styles specified"));
    }

    // Parse crop mode and target aspect from highlights or use defaults
    let crop_mode = vclip_models::CropMode::default();
    let target_aspect = vclip_models::AspectRatio::default();

    // Create and enqueue reprocess job
    let video_id_obj = vclip_models::VideoId::from_string(&video_id);
    let job = vclip_queue::ReprocessScenesJob::new(
        &user.uid,
        video_id_obj.clone(),
        request.scene_ids.clone(),
        styles,
    )
    .with_crop_mode(crop_mode)
    .with_target_aspect(target_aspect);
    
    let job_id = job.job_id.clone();

    // Enqueue the job
    state.queue.enqueue_reprocess(job).await
        .map_err(|e| ApiError::internal(format!("Failed to enqueue job: {}", e)))?;

    // Update video status to processing
    state
        .user_service
        .update_video_status(&user.uid, &video_id, vclip_models::VideoStatus::Processing)
        .await?;

    info!(
        "Reprocessing job {} enqueued for video {} by user {}: {} scenes, {} styles",
        job_id,
        video_id,
        user.uid,
        request.scene_ids.len(),
        request.styles.len()
    );

    Ok(Json(ReprocessScenesResponse {
        success: true,
        video_id: video_id.clone(),
        message: format!(
            "Reprocessing job enqueued for {} scene(s) with {} style(s). Connect via WebSocket to monitor progress.",
            request.scene_ids.len(),
            request.styles.len()
        ),
        job_id: Some(job_id.to_string()),
    }))
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Validate video ID format to prevent injection attacks.
/// 
/// Valid format: alphanumeric characters and hyphens only, 8-64 chars.
/// Examples: "9a4fef5b-e5b0-41c3-b64c-55ddb09346a3", "abc123-def456"
fn is_valid_video_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.len() < 8 {
        return false;
    }
    
    // Only allow alphanumeric and hyphens
    id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
