//! Video API handlers.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use vclip_models::{
    CreditContext, CreditOperationType, Style, VideoId, AspectRatio, CropMode, DetectionTier,
    ANALYSIS_CREDIT_COST,
};
use vclip_queue::ProcessVideoJob;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Processing progress response for frontend polling.
#[derive(Serialize)]
pub struct ProcessingProgressResponse {
    pub total_scenes: u32,
    pub completed_scenes: u32,
    pub total_clips: u32,
    pub completed_clips: u32,
    pub failed_clips: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_scene_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_scene_title: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Video info response.
#[derive(Serialize)]
pub struct VideoInfoResponse {
    pub id: String,
    /// Video status (pending, processing, completed, failed)
    pub status: String,
    /// Number of clips generated
    pub clip_count: u32,
    /// Processing progress (present only while processing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_progress: Option<ProcessingProgressResponse>,
    /// When the video was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When the video was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Video title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
    pub size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Whether this clip has a thumbnail available
    pub has_thumbnail: bool,
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

        // Format file size
        let size_mb = clip_meta.file_size_bytes as f64 / (1024.0 * 1024.0);
        let size_str = format!("{:.1} MB", size_mb);

        // Use filename if available, otherwise construct a nice filename from metadata
        // This handles legacy clips that may not have filename stored
        let name = if clip_meta.filename.is_empty() {
            // Generate a nice filename: clip_{priority:02}_{scene_id}_{sanitized_title}_{style}.mp4
            let safe_title = vclip_models::sanitize_filename_title(&clip_meta.scene_title);
            if safe_title.is_empty() {
                format!("clip_{:02}_{}_scene_{}.mp4", clip_meta.priority, clip_meta.scene_id, clip_meta.style)
            } else {
                format!("clip_{:02}_{}_{}.mp4", clip_meta.priority, safe_title, clip_meta.style)
            }
        } else {
            clip_meta.filename
        };

        clips.push(ClipInfo {
            clip_id: clip_meta.clip_id.clone(),
            name,
            title,
            description,
            size: size_str,
            style: Some(clip_meta.style),
            has_thumbnail: clip_meta.thumbnail_r2_key.is_some(),
            completed_at: clip_meta.completed_at.map(|dt| dt.to_rfc3339()),
            updated_at: clip_meta.updated_at.map(|dt| dt.to_rfc3339()),
        });
    }

    // Sort by name for consistent display
    clips.sort_by(|a, b| a.name.cmp(&b.name));

    // Convert processing progress to API response format
    let processing_progress = video_meta.processing_progress.as_ref().map(|p| {
        ProcessingProgressResponse {
            total_scenes: p.total_scenes,
            completed_scenes: p.completed_scenes,
            total_clips: p.total_clips,
            completed_clips: p.completed_clips,
            failed_clips: p.failed_clips,
            current_scene_id: p.current_scene_id,
            current_scene_title: p.current_scene_title.clone(),
            started_at: p.started_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
            error_message: p.error_message.clone(),
        }
    });

    Ok(Json(VideoInfoResponse {
        id: video_id,
        status: video_meta.status.as_str().to_string(),
        clip_count: clips.len() as u32,
        processing_progress,
        created_at: Some(video_meta.created_at.to_rfc3339()),
        updated_at: Some(video_meta.updated_at.to_rfc3339()),
        title: Some(video_meta.video_title.clone()),
        clips,
        custom_prompt: highlights.custom_prompt,
        video_title: highlights.video_title,
        video_url: highlights.video_url,
    }))
}

// Re-export from video_status module for backward compatibility
pub use super::video_status::{
    get_processing_status, is_valid_video_id, list_user_videos, ListVideosQuery,
    ProcessingStatusEntry, ProcessingStatusQuery, ProcessingStatusResponse, UserVideosResponse,
    VideoSummary,
};

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

    // Delete files from R2 (includes styled clips, raw segments, source video, neural cache)
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

    // Clear non-billable cache storage accounting (source video, raw segments, neural cache)
    let storage_repo = vclip_firestore::StorageAccountingRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    if let Err(e) = storage_repo.clear_video_cache().await {
        warn!(
            "Failed to clear cache storage accounting after deleting video {}: {}",
            video_id, e
        );
    }

    info!(
        "Deleted video {} for user {} ({} files, {} bytes, including cached data)",
        video_id, user.uid, files_deleted, video_size
    );

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
                info!("Deleted video {} for user {} ({} files, including cached data)", video_id, user.uid, files_deleted);
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

    // Clear non-billable cache storage accounting after bulk delete
    // Note: This clears ALL cache storage for the user, which is correct since
    // delete_video_files already deleted all cached data for each video
    if deleted_count > 0 {
        let storage_repo = vclip_firestore::StorageAccountingRepository::new(
            (*state.firestore).clone(),
            &user.uid,
        );
        if let Err(e) = storage_repo.clear_video_cache().await {
            warn!(
                "Failed to clear cache storage accounting after bulk delete for user {}: {}",
                user.uid, e
            );
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
// Update Clip Title
// ============================================================================

#[derive(Deserialize)]
pub struct UpdateClipTitleRequest {
    pub title: String,
}

#[derive(Serialize)]
pub struct UpdateClipTitleResponse {
    pub success: bool,
    pub clip_id: String,
    pub new_title: String,
}

/// Update clip title.
/// 
/// PATCH /api/videos/:video_id/clips/:clip_id/title
pub async fn update_clip_title(
    State(state): State<AppState>,
    Path((video_id, clip_id)): Path<(String, String)>,
    user: AuthUser,
    Json(request): Json<UpdateClipTitleRequest>,
) -> ApiResult<Json<UpdateClipTitleResponse>> {
    // Validate title
    let title = request.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }
    if title.len() > 200 {
        return Err(ApiError::bad_request("Title too long (max 200 characters)"));
    }

    // Verify ownership
    if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
        return Err(ApiError::not_found("Video not found"));
    }

    // Update clip title in Firestore
    let clip_repo = vclip_firestore::ClipRepository::new(
        (*state.firestore).clone(),
        &user.uid,
        vclip_models::VideoId::from_string(&video_id),
    );

    // Verify clip exists
    if clip_repo.get(&clip_id).await?.is_none() {
        return Err(ApiError::not_found("Clip not found"));
    }

    clip_repo.update_title(&clip_id, title).await.map_err(|e| {
        warn!("Failed to update clip title: {}", e);
        ApiError::internal(&format!("Failed to update clip title: {}", e))
    })?;

    info!(
        user_id = %user.uid,
        video_id = %video_id,
        clip_id = %clip_id,
        new_title = %title,
        "Updated clip title"
    );

    Ok(Json(UpdateClipTitleResponse {
        success: true,
        clip_id,
        new_title: title.to_string(),
    }))
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

    // Get clip_id before deleting metadata (needed for share cleanup)
    let clip_id = clip_repo.list(None).await
        .ok()
        .and_then(|clips| clips.into_iter().find(|c| c.filename == clip_name).map(|c| c.clip_id));

    // Delete clip metadata from Firestore
    let metadata_deleted = clip_repo.delete_by_filename(&clip_name).await?;

    // Clean up share slug if this clip had a share link
    if let Some(ref cid) = clip_id {
        let share_repo = vclip_firestore::ShareRepository::new((*state.firestore).clone());
        if let Err(e) = share_repo.delete_slug_for_clip(&user.uid, &video_id, cid).await {
            // Non-fatal: share may not exist
            warn!("Failed to delete share slug for clip {}: {}", cid, e);
        }
    }

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

/// Recompute clips_count and clips_by_style for a video and persist to Firestore.
async fn refresh_clips_count(state: &AppState, user_id: &str, video_id: &str) -> ApiResult<()> {
    let video_id_obj = vclip_models::VideoId::from_string(video_id);
    let video_repo = vclip_firestore::VideoRepository::new((*state.firestore).clone(), user_id);

    // This recalculates both clips_count and clips_by_style from actual clips
    video_repo
        .recalculate_clips_by_style(&video_id_obj)
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

    // Refresh video clips_count and clips_by_style to reflect deletions
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
    /// When true, overwrite existing clips instead of skipping them (default: false)
    #[serde(default)]
    pub overwrite: bool,
    /// Enable object detection for cinematic styles (default: false)
    #[serde(default)]
    pub enable_object_detection: bool,
    /// Enable Top Scenes compilation mode (default: false)
    #[serde(default)]
    pub top_scenes_compilation: bool,
    /// Cut silent parts from clips using VAD (default: false)
    #[serde(default)]
    pub cut_silent_parts: bool,
    /// Optional StreamerSplit parameters for user-controlled crop position/zoom.
    /// Only used when styles includes "streamer_split".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamer_split_params: Option<StreamerSplitParamsRequest>,
}

/// StreamerSplit parameters from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamerSplitParamsRequest {
    /// Horizontal position: "left", "center", or "right"
    #[serde(default = "default_position_x")]
    pub position_x: String,
    /// Vertical position: "top", "middle", or "bottom"
    #[serde(default = "default_position_y")]
    pub position_y: String,
    /// Zoom level (1.0 = full frame, 2.0 = 2x zoom, max 4.0)
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Optional static image URL to display instead of video crop
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_image_url: Option<String>,
    /// Optional manual crop region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_crop: Option<vclip_models::NormalizedRect>,
    /// Optional split ratio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_ratio: Option<f32>,
}

fn default_position_x() -> String { "left".to_string() }
fn default_position_y() -> String { "top".to_string() }
fn default_zoom() -> f32 { 1.5 }

/// Reprocess scenes response.
#[derive(Serialize)]
pub struct ReprocessScenesResponse {
    pub success: bool,
    pub video_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub total_clips: u32,
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

    // Parse styles with "all" expansion support
    let styles = Style::expand_styles(&request.styles);

    if styles.is_empty() {
        return Err(ApiError::bad_request("No valid styles specified"));
    }

    // Get plan limits for tier validation
    let limits = state.user_service.get_plan_limits(&user.uid).await?;

    // Validate detection tiers are allowed by the user's plan
    for style in &styles {
        let tier = style.detection_tier();
        if !limits.allows_detection_tier(tier) {
            let required_plan = match tier {
                DetectionTier::Cinematic => "Studio",
                DetectionTier::MotionAware | DetectionTier::SpeakerAware => "Pro",
                _ => "Pro",
            };
            return Err(ApiError::forbidden(format!(
                "Style '{}' requires a {} plan or higher. Please upgrade to access this feature.",
                style, required_plan
            )));
        }
    }

    // Check storage quota first (doesn't cost credits)
    let storage_usage = state.user_service.get_storage_usage(&user.uid).await?;
    if storage_usage.percentage() >= 100.0 {
        return Err(ApiError::forbidden(format!(
            "Storage limit exceeded. You've used {} of {} storage. Please delete some clips or upgrade your plan.",
            storage_usage.format_total(), storage_usage.format_limit()
        )));
    }

    // Fix #5: Load highlights and validate scene IDs BEFORE charging credits
    // This prevents unfair charges when scene IDs are invalid
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

    // Now that validation passed, calculate and charge credits
    let num_scenes = request.scene_ids.len() as u32;
    let num_styles = styles.len() as u32;
    let total_clips = num_scenes * num_styles;

    // Each scene × style combination costs credits based on the style's detection tier
    let mut total_credits: u32 = styles.iter().map(|s| s.credit_cost() * num_scenes).sum();
    
    // Add silent remover cost (+5 credits per scene) if enabled
    const SILENT_REMOVER_COST_PER_SCENE: u32 = 5;
    if request.cut_silent_parts {
        total_credits += SILENT_REMOVER_COST_PER_SCENE * num_scenes;
    }

    // Build description for credit transaction
    let styles_str: Vec<&str> = styles.iter().map(|s| s.as_filename_part()).collect();
    let description = format!(
        "Reprocess {} scene{} ({})",
        num_scenes,
        if num_scenes == 1 { "" } else { "s" },
        styles_str.join(", ")
    );

    // Build metadata for the transaction
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("scene_count".to_string(), num_scenes.to_string());
    metadata.insert("styles".to_string(), styles_str.join(","));
    if request.cut_silent_parts {
        metadata.insert("silent_remover".to_string(), "true".to_string());
    }

    let credit_context = CreditContext::new(
        CreditOperationType::Reprocessing,
        description,
    )
    .with_video_id(&video_id)
    .with_metadata(metadata);

    // Check and reserve credits (validates quota + charges upfront)
    state.user_service.check_and_reserve_credits_with_context(&user.uid, total_credits, credit_context).await?;

    info!(
        "Reserved {} credits for reprocessing {} scenes ({} clips) with {} styles for user {} (cut_silent_parts: {})",
        total_credits, num_scenes, total_clips, num_styles, user.uid, request.cut_silent_parts
    );

    // Parse crop mode and target aspect from highlights or use defaults
    let crop_mode = vclip_models::CropMode::default();
    let target_aspect = vclip_models::AspectRatio::default();

    // Convert StreamerSplit params from request to model type
    let streamer_split_params = request.streamer_split_params.as_ref().map(|p| {
        vclip_models::StreamerSplitParams {
            position_x: match p.position_x.as_str() {
                "left" => vclip_models::HorizontalPosition::Left,
                "right" => vclip_models::HorizontalPosition::Right,
                _ => vclip_models::HorizontalPosition::Center,
            },
            position_y: match p.position_y.as_str() {
                "top" => vclip_models::VerticalPosition::Top,
                "bottom" => vclip_models::VerticalPosition::Bottom,
                _ => vclip_models::VerticalPosition::Middle,
            },
            zoom: p.zoom,
            static_image_url: p.static_image_url.clone(),
            manual_crop: p.manual_crop,
            split_ratio: p.split_ratio,
        }
    });

    // Create and enqueue reprocess job
    let video_id_obj = vclip_models::VideoId::from_string(&video_id);
    let job = vclip_queue::ReprocessScenesJob::new(
        &user.uid,
        video_id_obj.clone(),
        request.scene_ids.clone(),
        styles,
    )
    .with_crop_mode(crop_mode)
    .with_target_aspect(target_aspect)
    .with_overwrite(request.overwrite)
    .with_streamer_split_params(streamer_split_params)
    .with_cut_silent_parts(request.cut_silent_parts)
    .with_object_detection(request.enable_object_detection)
    .with_top_scenes_compilation(request.top_scenes_compilation);
    
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
            "Reprocessing job enqueued for {} scene(s) with {} style(s). Refresh the page to check progress.",
            request.scene_ids.len(),
            request.styles.len()
        ),
        job_id: Some(job_id.to_string()),
        total_clips,
    }))
}

// ============================================================================
// Process Video (REST endpoint to replace WebSocket)
// ============================================================================

/// Request for processing a new video via REST API.
#[derive(Debug, Deserialize)]
pub struct ProcessVideoRequest {
    /// Video URL (YouTube, TikTok, etc.)
    pub url: String,
    /// Styles to apply
    #[serde(default)]
    pub styles: Vec<String>,
    /// Custom prompt for AI analysis
    #[serde(default)]
    pub prompt: Option<String>,
    /// Crop mode (none, auto, etc.)
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,
    /// Target aspect ratio
    #[serde(default = "default_target_aspect")]
    pub target_aspect: String,
}

fn default_crop_mode() -> String {
    "none".to_string()
}

fn default_target_aspect() -> String {
    "9:16".to_string()
}

/// Response for processing a new video.
#[derive(Serialize)]
pub struct ProcessVideoResponse {
    pub video_id: String,
    pub job_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Process a new video via REST API.
/// 
/// POST /api/videos/process
/// 
/// This replaces the WebSocket-based processing endpoint.
/// The job is enqueued and the client can poll for progress via Firebase.
pub async fn process_video(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<ProcessVideoRequest>,
) -> ApiResult<Json<ProcessVideoResponse>> {
    use crate::security::{validate_video_url, sanitize_string, MAX_PROMPT_LENGTH};

    // Validate video URL (SSRF protection)
    let validated_url = validate_video_url(&request.url)
        .into_result()
        .map_err(|e| ApiError::bad_request(format!("Invalid video URL: {}", e)))?;

    // Sanitize prompt
    let sanitized_prompt = request.prompt.as_ref().map(|p| {
        if p.len() > MAX_PROMPT_LENGTH {
            warn!(user = %user.uid, "Prompt truncated from {} to {} chars", p.len(), MAX_PROMPT_LENGTH);
        }
        sanitize_string(p)
    });

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&user.uid, user.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", user.uid, e);
    }

    // Get plan limits for tier validation
    let limits = state.user_service.get_plan_limits(&user.uid).await?;

    // Parse styles with "all" expansion support
    let style_strs = if request.styles.is_empty() {
        vec!["intelligent".to_string()]
    } else {
        request.styles.clone()
    };
    let styles = Style::expand_styles(&style_strs);

    if styles.is_empty() {
        return Err(ApiError::bad_request("No valid styles specified"));
    }

    // Validate detection tiers are allowed by the user's plan
    for style in &styles {
        let tier = style.detection_tier();
        if !limits.allows_detection_tier(tier) {
            let required_plan = match tier {
                DetectionTier::Cinematic => "Studio",
                DetectionTier::MotionAware | DetectionTier::SpeakerAware => "Pro",
                _ => "Pro", // Basic and above
            };
            return Err(ApiError::forbidden(format!(
                "Style '{}' requires a {} plan or higher. Please upgrade to access this feature.",
                style, required_plan
            )));
        }
    }

    // Check storage quota first (doesn't cost credits to check)
    let storage_usage = state.user_service.get_storage_usage(&user.uid).await?;
    if storage_usage.percentage() >= 100.0 {
        return Err(ApiError::forbidden(format!(
            "Storage limit exceeded. You've used {} of {} storage.",
            storage_usage.format_total(), storage_usage.format_limit()
        )));
    }

    // Video analysis costs credits - charge upfront
    // NOTE: This is the initial analysis job, not rendering. Analysis = 3 credits.
    let credit_context = CreditContext::new(
        CreditOperationType::Analysis,
        "Video analysis",
    );
    state.user_service.check_and_reserve_credits_with_context(&user.uid, ANALYSIS_CREDIT_COST, credit_context).await?;

    // Parse crop mode and target aspect
    let crop_mode: CropMode = request.crop_mode.parse().unwrap_or_default();
    let target_aspect: AspectRatio = request.target_aspect.parse().unwrap_or_default();

    // Create job
    let job = ProcessVideoJob::new(&user.uid, &validated_url, styles)
        .with_crop_mode(crop_mode)
        .with_target_aspect(target_aspect)
        .with_custom_prompt(sanitized_prompt.clone());
    
    let job_id = job.job_id.clone();
    let video_id = job.video_id.clone();

    // Create empty highlights document BEFORE enqueueing the job
    // This ensures the document exists when the user is redirected to the history page
    let empty_highlights = vclip_models::highlight::VideoHighlights {
        video_id: video_id.to_string(),
        highlights: vec![], // Empty - will be populated by worker after AI analysis
        video_url: Some(validated_url.clone()),
        video_title: None, // Will be set by worker after metadata fetch
        custom_prompt: sanitized_prompt,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    highlights_repo
        .upsert(&empty_highlights)
        .await
        .map_err(|e| {
            warn!("Failed to create empty highlights: {}", e);
            ApiError::internal(format!("Failed to initialize video: {}", e))
        })?;

    // Also create the video record so user_owns_video check passes
    // This record will be updated by the worker with the actual video title
    let video_meta = vclip_models::VideoMetadata::new(
        video_id.clone(),
        &user.uid,
        &validated_url,
        "Analyzing...", // Placeholder title until worker gets the real one
    );
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        &user.uid,
    );
    video_repo
        .create(&video_meta)
        .await
        .map_err(|e| {
            warn!("Failed to create video record: {}", e);
            ApiError::internal(format!("Failed to initialize video: {}", e))
        })?;

    // Enqueue job
    state.queue.enqueue_process(job).await
        .map_err(|e| ApiError::internal(format!("Failed to enqueue job: {}", e)))?;

    info!(
        "Process job {} enqueued for video {} by user {}",
        job_id, video_id, user.uid
    );

    Ok(Json(ProcessVideoResponse {
        video_id: video_id.to_string(),
        job_id: job_id.to_string(),
        status: "queued".to_string(),
        message: Some("Video submitted for processing. Refresh the page to check progress.".to_string()),
    }))
}

