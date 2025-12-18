//! Highlight management API handlers.
//!
//! This module provides endpoints for:
//! - Editing scene timestamps (FREE)
//! - Adding new scenes (single and bulk) (FREE)
//! - Generating more scenes via AI (3 credits)

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use vclip_firestore::{FromFirestoreValue, HighlightsRepository};
use vclip_models::{
    CreditContext, CreditOperationType, Highlight, HighlightCategory, VideoId,
    parse_timestamp, validate_timestamps, TimestampError,
};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::security::sanitize_string;
use crate::services::gemini::{
    build_existing_scenes_context, build_generate_more_prompt, get_fallback_base_prompt,
    GeminiClient,
};
use crate::state::AppState;

/// Maximum scenes allowed in a bulk add operation.
const MAX_BULK_ADD_SCENES: usize = 30;

/// Maximum scenes to generate via AI.
const MAX_GENERATE_SCENES: u32 = 10;

/// Credit cost for generating more scenes.
pub const GENERATE_MORE_SCENES_CREDIT_COST: u32 = 3;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert TimestampError to a user-friendly string for API responses.
fn timestamp_error_to_string(e: TimestampError) -> String {
    e.to_string()
}

/// Parse hook category string to HighlightCategory enum.
fn parse_hook_category(category: &str) -> Option<HighlightCategory> {
    match category.to_lowercase().as_str() {
        "emotional" => Some(HighlightCategory::Emotional),
        "educational" => Some(HighlightCategory::Educational),
        "controversial" => Some(HighlightCategory::Controversial),
        "inspirational" => Some(HighlightCategory::Inspirational),
        "humorous" => Some(HighlightCategory::Humorous),
        "dramatic" => Some(HighlightCategory::Dramatic),
        "surprising" => Some(HighlightCategory::Surprising),
        _ => Some(HighlightCategory::Other),
    }
}

/// Format HighlightCategory to lowercase string for API responses.
fn format_hook_category(category: &HighlightCategory) -> String {
    match category {
        HighlightCategory::Emotional => "emotional",
        HighlightCategory::Educational => "educational",
        HighlightCategory::Controversial => "controversial",
        HighlightCategory::Inspirational => "inspirational",
        HighlightCategory::Humorous => "humorous",
        HighlightCategory::Dramatic => "dramatic",
        HighlightCategory::Surprising => "surprising",
        HighlightCategory::Other => "other",
    }
    .to_string()
}

// ============================================================================
// Feature 1: Edit Scene Timestamps (FREE)
// ============================================================================

/// Request to update a scene's timestamps.
#[derive(Debug, Deserialize)]
pub struct UpdateSceneTimestampsRequest {
    /// New start timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub start: String,
    /// New end timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub end: String,
}

/// Response from updating a scene's timestamps.
#[derive(Serialize)]
pub struct UpdateSceneTimestampsResponse {
    pub success: bool,
    pub video_id: String,
    pub scene_id: u32,
    pub start: String,
    pub end: String,
    pub duration: u32,
}

/// Update a scene's timestamps.
///
/// PATCH /api/videos/:video_id/highlights/:scene_id
pub async fn update_scene_timestamps(
    State(state): State<AppState>,
    Path((video_id, scene_id)): Path<(String, u32)>,
    user: AuthUser,
    Json(request): Json<UpdateSceneTimestampsRequest>,
) -> ApiResult<Json<UpdateSceneTimestampsResponse>> {
    // Verify ownership
    if !state
        .user_service
        .user_owns_video(&user.uid, &video_id)
        .await?
    {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);
    let highlights_repo =
        HighlightsRepository::new((*state.firestore).clone(), &user.uid);

    // Get existing highlights
    let mut video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    // Find the scene to update
    let scene_idx = video_highlights
        .highlights
        .iter()
        .position(|h| h.id == scene_id)
        .ok_or_else(|| ApiError::not_found(format!("Scene {} not found", scene_id)))?;

    // Validate timestamps (no video duration check for now - could add if stored)
    let validated = validate_timestamps(&request.start, &request.end, None)
        .map_err(|e| ApiError::bad_request(timestamp_error_to_string(e)))?;

    // Update the scene
    video_highlights.highlights[scene_idx].start = validated.start.clone();
    video_highlights.highlights[scene_idx].end = validated.end.clone();
    video_highlights.highlights[scene_idx].duration = validated.duration_secs;
    video_highlights.updated_at = Utc::now();

    // Save to Firestore
    highlights_repo.upsert(&video_highlights).await?;

    info!(
        user_id = %user.uid,
        video_id = %video_id,
        scene_id = %scene_id,
        start = %validated.start,
        end = %validated.end,
        "Updated scene timestamps"
    );

    Ok(Json(UpdateSceneTimestampsResponse {
        success: true,
        video_id,
        scene_id,
        start: validated.start,
        end: validated.end,
        duration: validated.duration_secs,
    }))
}

// ============================================================================
// Feature 2: Add New Scenes (FREE)
// ============================================================================

/// Request to add a single scene.
#[derive(Debug, Deserialize)]
pub struct AddSceneRequest {
    /// Scene title (required)
    pub title: String,
    /// Why this is a good clip (required)
    pub reason: String,
    /// Start timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub start: String,
    /// End timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub end: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional hook category
    #[serde(default)]
    pub hook_category: Option<String>,
}

/// Response from adding a scene.
#[derive(Serialize)]
pub struct AddSceneResponse {
    pub success: bool,
    pub video_id: String,
    pub scene: SceneInfo,
}

/// Scene info for highlight management responses.
#[derive(Serialize, Clone)]
pub struct SceneInfo {
    pub id: u32,
    pub title: String,
    pub start: String,
    pub end: String,
    pub duration: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_category: Option<String>,
}

/// Add a single scene.
///
/// POST /api/videos/:video_id/highlights
pub async fn add_scene(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<AddSceneRequest>,
) -> ApiResult<Json<AddSceneResponse>> {
    // Validate input
    let title = sanitize_string(&request.title);
    let title = title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("Title is required"));
    }
    if title.len() > 200 {
        return Err(ApiError::bad_request("Title too long (max 200 characters)"));
    }

    let reason = sanitize_string(&request.reason);
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(ApiError::bad_request("Reason is required"));
    }
    if reason.len() > 500 {
        return Err(ApiError::bad_request("Reason too long (max 500 characters)"));
    }

    // Verify ownership
    if !state
        .user_service
        .user_owns_video(&user.uid, &video_id)
        .await?
    {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);
    let highlights_repo =
        HighlightsRepository::new((*state.firestore).clone(), &user.uid);

    // Get existing highlights
    let mut video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    // Validate timestamps
    let validated = validate_timestamps(&request.start, &request.end, None)
        .map_err(|e| ApiError::bad_request(timestamp_error_to_string(e)))?;

    // Calculate next ID (max existing + 1)
    let next_id = video_highlights
        .highlights
        .iter()
        .map(|h| h.id)
        .max()
        .unwrap_or(0)
        + 1;

    // Parse hook category
    let hook_category = request.hook_category.as_ref().and_then(|c| parse_hook_category(c));

    // Create new highlight
    let new_highlight = Highlight {
        id: next_id,
        title: title.to_string(),
        start: validated.start.clone(),
        end: validated.end.clone(),
        duration: validated.duration_secs,
        pad_before: 1.0,
        pad_after: 1.0,
        hook_category,
        reason: Some(reason.to_string()),
        description: request.description.as_ref().map(|d| sanitize_string(d)),
    };

    // Add to highlights and sort by start time
    video_highlights.highlights.push(new_highlight.clone());
    video_highlights.highlights.sort_by(|a, b| {
        let a_secs = parse_timestamp(&a.start).unwrap_or(0.0);
        let b_secs = parse_timestamp(&b.start).unwrap_or(0.0);
        a_secs.partial_cmp(&b_secs).unwrap_or(std::cmp::Ordering::Equal)
    });
    video_highlights.updated_at = Utc::now();

    // Save to Firestore
    highlights_repo.upsert(&video_highlights).await?;

    info!(
        user_id = %user.uid,
        video_id = %video_id,
        scene_id = %next_id,
        title = %title,
        "Added new scene"
    );

    Ok(Json(AddSceneResponse {
        success: true,
        video_id,
        scene: SceneInfo {
            id: new_highlight.id,
            title: new_highlight.title,
            start: new_highlight.start,
            end: new_highlight.end,
            duration: new_highlight.duration,
            reason: new_highlight.reason,
            description: new_highlight.description,
            hook_category: new_highlight.hook_category.as_ref().map(format_hook_category),
        },
    }))
}

// ============================================================================
// Bulk Add Scenes
// ============================================================================

/// Single scene entry for bulk add.
#[derive(Debug, Deserialize)]
pub struct BulkSceneEntry {
    pub title: String,
    pub reason: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hook_category: Option<String>,
}

/// Request to bulk add scenes.
#[derive(Debug, Deserialize)]
pub struct BulkAddScenesRequest {
    pub scenes: Vec<BulkSceneEntry>,
}

/// Validation error for a single scene.
#[derive(Serialize)]
pub struct SceneValidationError {
    pub index: usize,
    pub error: String,
}

/// Response from bulk adding scenes.
#[derive(Serialize)]
pub struct BulkAddScenesResponse {
    pub success: bool,
    pub video_id: String,
    pub added_count: u32,
    pub scenes: Vec<SceneInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SceneValidationError>,
}

/// Bulk add scenes.
///
/// POST /api/videos/:video_id/highlights/bulk
pub async fn bulk_add_scenes(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<BulkAddScenesRequest>,
) -> ApiResult<Json<BulkAddScenesResponse>> {
    // Validate count
    if request.scenes.is_empty() {
        return Err(ApiError::bad_request("At least one scene is required"));
    }
    if request.scenes.len() > MAX_BULK_ADD_SCENES {
        return Err(ApiError::bad_request(format!(
            "Cannot add more than {} scenes at once",
            MAX_BULK_ADD_SCENES
        )));
    }

    // Verify ownership
    if !state
        .user_service
        .user_owns_video(&user.uid, &video_id)
        .await?
    {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);
    let highlights_repo =
        HighlightsRepository::new((*state.firestore).clone(), &user.uid);

    // Get existing highlights
    let mut video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    // Determine starting ID
    let mut next_id = video_highlights
        .highlights
        .iter()
        .map(|h| h.id)
        .max()
        .unwrap_or(0)
        + 1;

    let mut added_scenes = Vec::new();
    let mut errors = Vec::new();

    for (index, entry) in request.scenes.iter().enumerate() {
        // Validate title
        let title = sanitize_string(&entry.title);
        let title = title.trim();
        if title.is_empty() {
            errors.push(SceneValidationError {
                index,
                error: "Title is required".to_string(),
            });
            continue;
        }
        if title.len() > 200 {
            errors.push(SceneValidationError {
                index,
                error: "Title too long (max 200 characters)".to_string(),
            });
            continue;
        }

        // Validate reason
        let reason = sanitize_string(&entry.reason);
        let reason = reason.trim();
        if reason.is_empty() {
            errors.push(SceneValidationError {
                index,
                error: "Reason is required".to_string(),
            });
            continue;
        }
        if reason.len() > 500 {
            errors.push(SceneValidationError {
                index,
                error: "Reason too long (max 500 characters)".to_string(),
            });
            continue;
        }

        // Validate timestamps
        let validated = match validate_timestamps(&entry.start, &entry.end, None) {
            Ok(result) => result,
            Err(e) => {
                errors.push(SceneValidationError {
                    index,
                    error: timestamp_error_to_string(e),
                });
                continue;
            }
        };

        // Parse hook category
        let hook_category = entry.hook_category.as_ref().and_then(|c| parse_hook_category(c));

        // Create highlight
        let new_highlight = Highlight {
            id: next_id,
            title: title.to_string(),
            start: validated.start,
            end: validated.end,
            duration: validated.duration_secs,
            pad_before: 1.0,
            pad_after: 1.0,
            hook_category,
            reason: Some(reason.to_string()),
            description: entry.description.as_ref().map(|d| sanitize_string(d)),
        };

        added_scenes.push(SceneInfo {
            id: new_highlight.id,
            title: new_highlight.title.clone(),
            start: new_highlight.start.clone(),
            end: new_highlight.end.clone(),
            duration: new_highlight.duration,
            reason: new_highlight.reason.clone(),
            description: new_highlight.description.clone(),
            hook_category: new_highlight.hook_category.as_ref().map(format_hook_category),
        });

        video_highlights.highlights.push(new_highlight);
        next_id += 1;
    }

    // Sort by start time
    video_highlights.highlights.sort_by(|a, b| {
        let a_secs = parse_timestamp(&a.start).unwrap_or(0.0);
        let b_secs = parse_timestamp(&b.start).unwrap_or(0.0);
        a_secs.partial_cmp(&b_secs).unwrap_or(std::cmp::Ordering::Equal)
    });
    video_highlights.updated_at = Utc::now();

    // Save if any scenes were added
    if !added_scenes.is_empty() {
        highlights_repo.upsert(&video_highlights).await?;
    }

    let added_count = added_scenes.len() as u32;
    info!(
        user_id = %user.uid,
        video_id = %video_id,
        added_count = %added_count,
        error_count = %errors.len(),
        "Bulk added scenes"
    );

    Ok(Json(BulkAddScenesResponse {
        success: added_count > 0,
        video_id,
        added_count,
        scenes: added_scenes,
        errors,
    }))
}

// ============================================================================
// Feature 3: Generate More Scenes (3 credits)
// ============================================================================

/// Request to generate more scenes.
#[derive(Debug, Deserialize)]
pub struct GenerateMoreScenesRequest {
    /// Number of scenes to generate (default: 10, max: 10)
    #[serde(default = "default_generate_count")]
    pub count: u32,
    /// Client-generated idempotency key to prevent double-charging
    pub idempotency_key: String,
}

fn default_generate_count() -> u32 {
    10
}

/// Response from generating more scenes.
#[derive(Serialize)]
pub struct GenerateMoreScenesResponse {
    pub success: bool,
    pub video_id: String,
    pub generated_count: u32,
    pub scenes: Vec<SceneInfo>,
    pub credits_charged: u32,
}

/// Generate more scenes using AI.
///
/// POST /api/videos/:video_id/highlights/generate-more
pub async fn generate_more_scenes(
    State(state): State<AppState>,
    Path(video_id): Path<String>,
    user: AuthUser,
    Json(request): Json<GenerateMoreScenesRequest>,
) -> ApiResult<Json<GenerateMoreScenesResponse>> {
    // Validate request
    let count = request.count.min(MAX_GENERATE_SCENES);
    if count == 0 {
        return Err(ApiError::bad_request("Count must be at least 1"));
    }

    if request.idempotency_key.is_empty() {
        return Err(ApiError::bad_request("Idempotency key is required"));
    }

    // Verify ownership
    if !state
        .user_service
        .user_owns_video(&user.uid, &video_id)
        .await?
    {
        return Err(ApiError::not_found("Video not found"));
    }

    // Check idempotency (5 minute TTL)
    let idempotency_key = format!(
        "generate-more:{}:{}:{}",
        user.uid, video_id, request.idempotency_key
    );
    let acquired = state
        .queue
        .try_acquire_idempotency(&idempotency_key, 300)
        .await
        .map_err(|e| {
            warn!("Failed to check idempotency: {}", e);
            ApiError::internal("Failed to process request")
        })?;

    if !acquired {
        return Err(ApiError::Conflict(
            "This request is already being processed. Please wait.".to_string(),
        ));
    }

    let video_id_obj = VideoId::from_string(&video_id);
    let highlights_repo =
        HighlightsRepository::new((*state.firestore).clone(), &user.uid);

    // Get existing highlights
    let mut video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    // Get video URL
    let video_url = video_highlights.video_url.as_ref().ok_or_else(|| {
        ApiError::bad_request("Video URL not found. Cannot generate more scenes.")
    })?;

    // Build credit context
    let mut metadata = HashMap::new();
    metadata.insert("video_id".to_string(), video_id.clone());
    metadata.insert("count".to_string(), count.to_string());

    let credit_context = CreditContext::new(
        CreditOperationType::GenerateMoreScenes,
        format!("Generate {} more scenes", count),
    )
    .with_video_id(&video_id)
    .with_metadata(metadata);

    // Reserve credits upfront
    state
        .user_service
        .check_and_reserve_credits_with_context(
            &user.uid,
            GENERATE_MORE_SCENES_CREDIT_COST,
            credit_context,
        )
        .await?;

    info!(
        user_id = %user.uid,
        video_id = %video_id,
        credits = %GENERATE_MORE_SCENES_CREDIT_COST,
        "Reserved credits for generate more scenes"
    );

    // Get admin base prompt
    let admin_prompt = get_admin_base_prompt_from_firestore(&state).await;

    // Build prompt with existing scenes context
    let existing_scenes_context = build_existing_scenes_context(&video_highlights.highlights);
    let user_prompt = video_highlights.custom_prompt.clone().unwrap_or_default();

    let generate_more_prompt = build_generate_more_prompt(
        &admin_prompt,
        &user_prompt,
        &existing_scenes_context,
        count,
    );

    // Create Gemini client and generate
    let gemini_client = GeminiClient::new()?;

    let ai_response = gemini_client
        .generate_more_scenes(&generate_more_prompt, video_url)
        .await
        .map_err(|e| {
            warn!("AI analysis failed for {}: {:?}", video_id, e);
            ApiError::internal("AI analysis failed. Please try again later.")
        })?;

    // Get max existing ID
    let mut next_id = video_highlights
        .highlights
        .iter()
        .map(|h| h.id)
        .max()
        .unwrap_or(0)
        + 1;

    // Filter out scenes that overlap with existing ones
    let existing_starts: Vec<f64> = video_highlights
        .highlights
        .iter()
        .filter_map(|h| parse_timestamp(&h.start).ok())
        .collect();

    let mut new_scenes = Vec::new();
    for ai_highlight in ai_response.highlights {
        // Check if this scene overlaps significantly with existing
        let new_start = parse_timestamp(&ai_highlight.start).unwrap_or(0.0);
        let is_duplicate = existing_starts.iter().any(|&existing_start| {
            (new_start - existing_start).abs() < 5.0 // Within 5 seconds = duplicate
        });

        if is_duplicate {
            continue;
        }

        // Validate timestamps
        let validated = match validate_timestamps(&ai_highlight.start, &ai_highlight.end, None) {
            Ok(result) => result,
            Err(_) => continue, // Skip invalid timestamps
        };

        // Parse hook category
        let hook_category = ai_highlight.hook_category.as_ref().and_then(|c| parse_hook_category(c));

        let new_highlight = Highlight {
            id: next_id,
            title: ai_highlight.title,
            start: validated.start.clone(),
            end: validated.end.clone(),
            duration: validated.duration_secs,
            pad_before: ai_highlight.pad_before_seconds,
            pad_after: ai_highlight.pad_after_seconds,
            hook_category,
            reason: ai_highlight.reason,
            description: ai_highlight.description,
        };

        new_scenes.push(SceneInfo {
            id: new_highlight.id,
            title: new_highlight.title.clone(),
            start: new_highlight.start.clone(),
            end: new_highlight.end.clone(),
            duration: new_highlight.duration,
            reason: new_highlight.reason.clone(),
            description: new_highlight.description.clone(),
            hook_category: new_highlight.hook_category.as_ref().map(format_hook_category),
        });

        video_highlights.highlights.push(new_highlight);
        next_id += 1;

        // Stop if we've generated enough
        if new_scenes.len() >= count as usize {
            break;
        }
    }

    // Sort by start time
    video_highlights.highlights.sort_by(|a, b| {
        let a_secs = parse_timestamp(&a.start).unwrap_or(0.0);
        let b_secs = parse_timestamp(&b.start).unwrap_or(0.0);
        a_secs.partial_cmp(&b_secs).unwrap_or(std::cmp::Ordering::Equal)
    });
    video_highlights.updated_at = Utc::now();

    // Save to Firestore
    highlights_repo.upsert(&video_highlights).await?;

    let generated_count = new_scenes.len() as u32;
    info!(
        user_id = %user.uid,
        video_id = %video_id,
        generated_count = %generated_count,
        credits_charged = %GENERATE_MORE_SCENES_CREDIT_COST,
        "Generated more scenes"
    );

    Ok(Json(GenerateMoreScenesResponse {
        success: true,
        video_id,
        generated_count,
        scenes: new_scenes,
        credits_charged: GENERATE_MORE_SCENES_CREDIT_COST,
    }))
}

/// Get the admin base prompt from Firestore.
async fn get_admin_base_prompt_from_firestore(state: &AppState) -> String {
    let doc = match state.firestore.get_document("admin", "config").await {
        Ok(Some(d)) => d,
        _ => return get_fallback_base_prompt(),
    };

    doc.fields
        .as_ref()
        .and_then(|fields| fields.get("base_prompt"))
        .and_then(|v| String::from_firestore_value(v))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(get_fallback_base_prompt)
}

// ============================================================================
// Delete Scene
// ============================================================================

/// Response from deleting a scene.
#[derive(Serialize)]
pub struct DeleteSceneResponse {
    pub success: bool,
    pub video_id: String,
    pub scene_id: u32,
}

/// Delete a scene from highlights.
///
/// DELETE /api/videos/:video_id/highlights/:scene_id
pub async fn delete_scene(
    State(state): State<AppState>,
    Path((video_id, scene_id)): Path<(String, u32)>,
    user: AuthUser,
) -> ApiResult<Json<DeleteSceneResponse>> {
    // Verify ownership
    if !state
        .user_service
        .user_owns_video(&user.uid, &video_id)
        .await?
    {
        return Err(ApiError::not_found("Video not found"));
    }

    let video_id_obj = VideoId::from_string(&video_id);
    let highlights_repo =
        HighlightsRepository::new((*state.firestore).clone(), &user.uid);

    // Get existing highlights
    let mut video_highlights = highlights_repo
        .get(&video_id_obj)
        .await?
        .ok_or_else(|| ApiError::not_found("Highlights not found for this video"))?;

    // Find and remove the scene
    let original_len = video_highlights.highlights.len();
    video_highlights.highlights.retain(|h| h.id != scene_id);

    if video_highlights.highlights.len() == original_len {
        return Err(ApiError::not_found(format!("Scene {} not found", scene_id)));
    }

    video_highlights.updated_at = Utc::now();

    // Save to Firestore
    highlights_repo.upsert(&video_highlights).await?;

    info!(
        user_id = %user.uid,
        video_id = %video_id,
        scene_id = %scene_id,
        "Deleted scene"
    );

    Ok(Json(DeleteSceneResponse {
        success: true,
        video_id,
        scene_id,
    }))
}
