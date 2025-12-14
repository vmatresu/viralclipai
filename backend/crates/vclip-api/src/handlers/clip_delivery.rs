//! Clip delivery handlers.
//!
//! Secure endpoints for clip playback, download, and sharing.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use vclip_firestore::{ClipRepository, ShareRepository};
use vclip_models::{
    ClipStatus, CreateShareRequest, ShareConfig, ShareResponse,
    is_valid_share_slug, MAX_SHARE_EXPIRY_HOURS,
};
use vclip_storage::{DeliveryConfig, DeliveryUrl, DeliveryUrlGenerator};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::security::is_valid_clip_name;
use crate::state::AppState;

// ============================================================================
// Response Types
// ============================================================================

/// Response for playback/download URL requests.
#[derive(Debug, Serialize)]
pub struct PlaybackUrlResponse {
    /// The delivery URL.
    pub url: String,
    /// When this URL expires (ISO 8601).
    pub expires_at: String,
    /// Expiry in seconds from now.
    pub expires_in_secs: u64,
    /// Content type.
    pub content_type: String,
    /// Clip metadata.
    pub clip: ClipSummary,
}

/// Summary of clip metadata for delivery responses.
#[derive(Debug, Serialize)]
pub struct ClipSummary {
    pub clip_id: String,
    pub filename: String,
    pub title: String,
    pub duration_seconds: f64,
    pub file_size_bytes: u64,
}

/// Request body for download URL (optional filename override).
#[derive(Debug, Deserialize)]
pub struct DownloadUrlRequest {
    /// Custom filename for the download.
    #[serde(default)]
    pub filename: Option<String>,
}

// ============================================================================
// Playback URL Handler
// ============================================================================

/// Generate a short-lived playback URL for a clip.
///
/// POST /api/clips/{clip_id}/play-url
///
/// Response:
/// ```json
/// {
///   "url": "https://...",
///   "expires_at": "2024-01-01T12:15:00Z",
///   "expires_in_secs": 900,
///   "content_type": "video/mp4",
///   "clip": { ... }
/// }
/// ```
pub async fn get_playback_url(
    State(state): State<AppState>,
    Path(clip_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<PlaybackUrlResponse>> {
    // Validate clip_id format
    if !is_valid_clip_name(&clip_id) {
        return Err(ApiError::bad_request("Invalid clip ID format"));
    }

    // Look up clip metadata
    let clip = find_clip_by_id(&state, &user.uid, &clip_id).await?;

    // Verify ownership
    if clip.user_id != user.uid {
        return Err(ApiError::forbidden("You don't own this clip"));
    }

    // Generate delivery URL
    let delivery_config = DeliveryConfig::from_env();
    let generator = DeliveryUrlGenerator::new((*state.storage).clone(), delivery_config);

    let delivery_url = generator
        .playback_url(&clip.r2_key, &clip.clip_id, &user.uid)
        .await
        .map_err(|e| {
            warn!(clip_id = %clip_id, error = %e, "Failed to generate playback URL");
            ApiError::internal("Failed to generate playback URL")
        })?;

    info!(clip_id = %clip_id, user_id = %user.uid, "Generated playback URL");

    Ok(Json(PlaybackUrlResponse {
        url: delivery_url.url,
        expires_at: delivery_url.expires_at,
        expires_in_secs: delivery_url.expires_in_secs,
        content_type: delivery_url.content_type,
        clip: ClipSummary {
            clip_id: clip.clip_id,
            filename: clip.filename,
            title: clip.scene_title,
            duration_seconds: clip.duration_seconds,
            file_size_bytes: clip.file_size_bytes,
        },
    }))
}

// ============================================================================
// Download URL Handler
// ============================================================================

/// Generate a short-lived download URL for a clip.
///
/// POST /api/clips/{clip_id}/download-url
///
/// Request body (optional):
/// ```json
/// { "filename": "my-custom-filename.mp4" }
/// ```
///
/// Response: Same as playback URL.
pub async fn get_download_url(
    State(state): State<AppState>,
    Path(clip_id): Path<String>,
    user: AuthUser,
    Json(body): Json<Option<DownloadUrlRequest>>,
) -> ApiResult<Json<PlaybackUrlResponse>> {
    // Validate clip_id format
    if !is_valid_clip_name(&clip_id) {
        return Err(ApiError::bad_request("Invalid clip ID format"));
    }

    // Look up clip metadata
    let clip = find_clip_by_id(&state, &user.uid, &clip_id).await?;

    // Verify ownership
    if clip.user_id != user.uid {
        return Err(ApiError::forbidden("You don't own this clip"));
    }

    // Determine filename for Content-Disposition
    let filename = body
        .as_ref()
        .and_then(|b| b.filename.as_deref())
        .map(sanitize_download_filename)
        .unwrap_or_else(|| clip.filename.clone());

    // Generate delivery URL
    let delivery_config = DeliveryConfig::from_env();
    let generator = DeliveryUrlGenerator::new((*state.storage).clone(), delivery_config);

    let delivery_url = generator
        .download_url(&clip.r2_key, &clip.clip_id, &user.uid, Some(&filename))
        .await
        .map_err(|e| {
            warn!(clip_id = %clip_id, error = %e, "Failed to generate download URL");
            ApiError::internal("Failed to generate download URL")
        })?;

    info!(clip_id = %clip_id, user_id = %user.uid, "Generated download URL");

    Ok(Json(PlaybackUrlResponse {
        url: delivery_url.url,
        expires_at: delivery_url.expires_at,
        expires_in_secs: delivery_url.expires_in_secs,
        content_type: delivery_url.content_type,
        clip: ClipSummary {
            clip_id: clip.clip_id,
            filename: clip.filename,
            title: clip.scene_title,
            duration_seconds: clip.duration_seconds,
            file_size_bytes: clip.file_size_bytes,
        },
    }))
}

// ============================================================================
// Thumbnail URL Handler
// ============================================================================

/// Generate a short-lived thumbnail URL for a clip.
///
/// POST /api/clips/{clip_id}/thumbnail-url
pub async fn get_thumbnail_url(
    State(state): State<AppState>,
    Path(clip_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<DeliveryUrl>> {
    // Validate clip_id format
    if !is_valid_clip_name(&clip_id) {
        return Err(ApiError::bad_request("Invalid clip ID format"));
    }

    // Look up clip metadata
    let clip = find_clip_by_id(&state, &user.uid, &clip_id).await?;

    // Verify ownership
    if clip.user_id != user.uid {
        return Err(ApiError::forbidden("You don't own this clip"));
    }

    // Check if thumbnail exists
    let thumb_key = clip.thumbnail_r2_key.as_ref().ok_or_else(|| {
        ApiError::not_found("Clip has no thumbnail")
    })?;

    // Generate delivery URL
    let delivery_config = DeliveryConfig::from_env();
    let generator = DeliveryUrlGenerator::new((*state.storage).clone(), delivery_config);

    let delivery_url = generator
        .thumbnail_url(thumb_key, &clip.clip_id, &user.uid)
        .await
        .map_err(|e| {
            warn!(clip_id = %clip_id, error = %e, "Failed to generate thumbnail URL");
            ApiError::internal("Failed to generate thumbnail URL")
        })?;

    Ok(Json(delivery_url))
}

// ============================================================================
// Share Handlers
// ============================================================================

/// Create or update a share link for a clip.
///
/// POST /api/clips/{clip_id}/share
///
/// Request body:
/// ```json
/// {
///   "access_level": "view_playback",
///   "expires_in_hours": 24,
///   "watermark_enabled": false
/// }
/// ```
///
/// Response:
/// ```json
/// {
///   "share_url": "https://viralclipai.io/c/abc123xyz",
///   "share_slug": "abc123xyz",
///   "access_level": "view_playback",
///   "expires_at": "2024-01-02T12:00:00Z",
///   "watermark_enabled": false,
///   "created_at": "2024-01-01T12:00:00Z"
/// }
/// ```
pub async fn create_share(
    State(state): State<AppState>,
    Path(clip_id): Path<String>,
    user: AuthUser,
    Json(body): Json<CreateShareRequest>,
) -> ApiResult<Json<ShareResponse>> {
    // Validate clip_id format
    if !is_valid_clip_name(&clip_id) {
        return Err(ApiError::bad_request("Invalid clip ID format"));
    }

    // Look up clip metadata
    let clip = find_clip_by_id(&state, &user.uid, &clip_id).await?;

    // Verify ownership
    if clip.user_id != user.uid {
        return Err(ApiError::forbidden("Only the owner can share this clip"));
    }

    // Validate expiry if specified (max 30 days)
    if let Some(hours) = body.expires_in_hours {
        if hours > MAX_SHARE_EXPIRY_HOURS {
            return Err(ApiError::bad_request(format!(
                "Expiry cannot exceed {} hours ({} days)",
                MAX_SHARE_EXPIRY_HOURS,
                MAX_SHARE_EXPIRY_HOURS / 24
            )));
        }
    }

    // Create share config
    let mut share_config = ShareConfig::new(
        &clip.clip_id,
        &user.uid,
        clip.video_id.as_str(),
        body.access_level,
    );

    // Set expiry if specified
    if let Some(hours) = body.expires_in_hours {
        let expires_at = Utc::now() + ChronoDuration::hours(hours as i64);
        share_config = share_config.with_expiry(expires_at);
    }

    // Set watermark if enabled
    if body.watermark_enabled {
        share_config = share_config.with_watermark();
    }

    // Persist share config to Firestore (dual-document pattern)
    let share_repo = ShareRepository::new((*state.firestore).clone());
    share_repo.create_share(&share_config).await.map_err(|e| {
        warn!(clip_id = %clip_id, error = %e, "Failed to persist share config");
        ApiError::internal("Failed to create share link")
    })?;

    let base_url = std::env::var("PUBLIC_APP_URL")
        .unwrap_or_else(|_| "https://viralclipai.io".to_string());

    let response = ShareResponse::from_config(&share_config, &base_url);

    info!(
        clip_id = %clip_id,
        user_id = %user.uid,
        share_slug = %share_config.share_slug,
        "Created share link"
    );

    Ok(Json(response))
}

/// Revoke a share link for a clip.
///
/// DELETE /api/clips/{clip_id}/share
///
/// Response: 204 No Content on success.
pub async fn revoke_share(
    State(state): State<AppState>,
    Path(clip_id): Path<String>,
    user: AuthUser,
) -> ApiResult<StatusCode> {
    // Validate clip_id format
    if !is_valid_clip_name(&clip_id) {
        return Err(ApiError::bad_request("Invalid clip ID format"));
    }

    // Look up clip metadata
    let clip = find_clip_by_id(&state, &user.uid, &clip_id).await?;

    // Verify ownership
    if clip.user_id != user.uid {
        return Err(ApiError::forbidden("Only the owner can revoke sharing"));
    }

    // Get the share config to find the slug
    let share_repo = ShareRepository::new((*state.firestore).clone());
    let share_config = share_repo
        .get_config(&user.uid, clip.video_id.as_str(), &clip_id)
        .await
        .map_err(|e| {
            warn!(clip_id = %clip_id, error = %e, "Failed to look up share config");
            ApiError::internal("Database error")
        })?;

    // If no share config exists, return success (idempotent)
    let config = match share_config {
        Some(c) => c,
        None => {
            info!(clip_id = %clip_id, user_id = %user.uid, "No active share to revoke");
            return Ok(StatusCode::NO_CONTENT);
        }
    };

    // Disable the share (updates config and deletes slug index)
    share_repo
        .disable_share(&user.uid, clip.video_id.as_str(), &clip_id, &config.share_slug)
        .await
        .map_err(|e| {
            warn!(clip_id = %clip_id, error = %e, "Failed to revoke share");
            ApiError::internal("Failed to revoke share link")
        })?;

    info!(
        clip_id = %clip_id,
        user_id = %user.uid,
        share_slug = %config.share_slug,
        "Revoked share link"
    );

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Public Share Resolution (for /c/{share_slug} redirect)
// ============================================================================

/// Resolve a share slug to a playback redirect.
///
/// GET /c/{share_slug}
///
/// Returns: 302 redirect to a short-lived presigned URL, or error.
pub async fn resolve_share(
    State(state): State<AppState>,
    Path(share_slug): Path<String>,
) -> Result<Response, ApiError> {
    use axum::http::header;
    use axum::response::IntoResponse;
    use std::time::Duration;

    // Validate share_slug format
    if !is_valid_share_slug(&share_slug) {
        return Err(ApiError::bad_request("Invalid share link"));
    }

    // Look up share by slug
    let share_repo = ShareRepository::new((*state.firestore).clone());
    let slug_info = share_repo.get_by_slug(&share_slug).await.map_err(|e| {
        warn!(share_slug = %share_slug, error = %e, "Failed to look up share slug");
        ApiError::internal("Database error")
    })?;

    // Check if share exists
    let slug_info = match slug_info {
        Some(info) => info,
        None => {
            info!(share_slug = %share_slug, "Share slug not found");
            return Err(ApiError::NotFound("Share link not found".to_string()));
        }
    };

    // Check if share is disabled
    if slug_info.disabled_at.is_some() {
        info!(share_slug = %share_slug, "Share has been revoked");
        return Err(ApiError::Gone("Share link has been revoked".to_string()));
    }

    // Check if share is expired
    if let Some(expires_at) = slug_info.expires_at {
        if Utc::now() > expires_at {
            info!(share_slug = %share_slug, "Share has expired");
            return Err(ApiError::Gone("Share link has expired".to_string()));
        }
    }

    // Look up clip metadata to get R2 key
    let clip = find_clip_by_owner_context(
        &state,
        &slug_info.user_id,
        &slug_info.video_id,
        &slug_info.clip_id,
    )
    .await?;

    // Generate short-lived delivery URL (1 hour)
    let delivery_config = DeliveryConfig::from_env();
    let generator = DeliveryUrlGenerator::new((*state.storage).clone(), delivery_config.clone());

    // Use Worker URL if configured, otherwise presigned URL
    let delivery_url = if delivery_config.should_use_worker() {
        generator
            .generate_worker_url_with_key(
                &clip.clip_id,
                &slug_info.user_id,
                Some(&clip.r2_key),
                vclip_storage::DeliveryScope::Playback,
                Duration::from_secs(3600), // 1 hour
            )
            .map_err(|e| {
                warn!(share_slug = %share_slug, error = %e, "Failed to generate worker URL");
                ApiError::internal("Failed to generate playback URL")
            })?
    } else {
        generator
            .playback_url(&clip.r2_key, &clip.clip_id, &slug_info.user_id)
            .await
            .map_err(|e| {
                warn!(share_slug = %share_slug, error = %e, "Failed to generate playback URL");
                ApiError::internal("Failed to generate playback URL")
            })?
    };

    info!(
        share_slug = %share_slug,
        clip_id = %clip.clip_id,
        user_id = %slug_info.user_id,
        "Resolved share link"
    );

    // Build 302 redirect response with cache control
    let response = (
        StatusCode::FOUND,
        [
            (header::LOCATION, delivery_url.url),
            // Prevent caching so revocation takes effect quickly
            (header::CACHE_CONTROL, "private, max-age=60".to_string()),
        ],
    )
        .into_response();

    Ok(response)
}

/// Find a clip by owner context (user_id, video_id, clip_id).
async fn find_clip_by_owner_context(
    state: &AppState,
    user_id: &str,
    video_id: &str,
    clip_id: &str,
) -> ApiResult<vclip_models::ClipMetadata> {
    let video_id_obj = vclip_models::VideoId::from_string(video_id.to_string());
    let clip_repo = ClipRepository::new((*state.firestore).clone(), user_id, video_id_obj);

    let clips = clip_repo.list(Some(ClipStatus::Completed)).await.map_err(|e| {
        warn!(video_id = %video_id, error = %e, "Failed to list clips");
        ApiError::internal("Database error")
    })?;

    clips
        .into_iter()
        .find(|c| c.clip_id == clip_id)
        .ok_or_else(|| ApiError::not_found("Clip not found"))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find a clip by ID, searching across all user's videos.
///
/// This is a temporary implementation that lists all videos and clips.
/// In a production system, we'd have a direct clip lookup by ID.
async fn find_clip_by_id(
    state: &AppState,
    user_id: &str,
    clip_id: &str,
) -> ApiResult<vclip_models::ClipMetadata> {
    // List user's videos
    let video_repo = vclip_firestore::VideoRepository::new(
        (*state.firestore).clone(),
        user_id,
    );

    let videos = video_repo.list(Some(100)).await.map_err(|e| {
        warn!(user_id = %user_id, error = %e, "Failed to list videos");
        ApiError::internal("Database error")
    })?;

    // Search clips in each video
    for video in videos {
        let clip_repo = ClipRepository::new(
            (*state.firestore).clone(),
            user_id,
            video.video_id.clone(),
        );

        let clips = clip_repo.list(Some(ClipStatus::Completed)).await.map_err(|e| {
            warn!(video_id = %video.video_id, error = %e, "Failed to list clips");
            ApiError::internal("Database error")
        })?;

        if let Some(clip) = clips.into_iter().find(|c| c.clip_id == clip_id) {
            return Ok(clip);
        }
    }

    Err(ApiError::not_found("Clip not found"))
}

/// Sanitize a download filename.
fn sanitize_download_filename(name: &str) -> String {
    let safe_joined: String = name
        .split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect::<Vec<_>>()
        .join("");

    let sanitized: String = safe_joined
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(100)
        .collect();

    if sanitized.is_empty() {
        "clip.mp4".to_string()
    } else if !sanitized.ends_with(".mp4") {
        format!("{}.mp4", sanitized)
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_download_filename() {
        assert_eq!(sanitize_download_filename("my-video.mp4"), "my-video.mp4");
        assert_eq!(sanitize_download_filename("my video"), "myvideo.mp4");
        assert_eq!(sanitize_download_filename(""), "clip.mp4");
        assert_eq!(sanitize_download_filename("../../../etc/passwd"), "etcpasswd.mp4");
    }
}
