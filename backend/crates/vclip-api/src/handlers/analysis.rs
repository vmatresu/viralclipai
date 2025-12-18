//! Analysis workflow API handlers.
//!
//! These handlers support the two-step video analysis workflow:
//! 1. Analyze: Start an async analysis job, returns job_id and draft_id
//! 2. Select & Process: Submit selected scenes for rendering

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use vclip_firestore::AnalysisDraftRepository;
use vclip_models::{
    AnalysisDraft, AnalysisStatus, AnalysisStatusResponse, CreditContext, CreditOperationType,
    DetectionTier, DraftScene, ProcessDraftRequest, ProcessingEstimate, StartAnalysisResponse,
    Style, ANALYSIS_CREDIT_COST,
};
use vclip_queue::{AnalyzeVideoJob, RenderSceneStyleJob};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::security::{sanitize_string, validate_video_url};
use crate::state::AppState;

/// TTL in days for analysis drafts by plan tier.
const FREE_DRAFT_TTL_DAYS: i64 = 7;
const PAID_DRAFT_TTL_DAYS: i64 = 30;

// ============================================================================
// Start Analysis
// ============================================================================

/// Request to start video analysis.
#[derive(Debug, Deserialize)]
pub struct StartAnalysisRequest {
    /// YouTube URL to analyze
    pub url: String,
    /// Optional AI instructions
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Start an async video analysis job.
///
/// Creates an AnalysisDraft record and enqueues an AnalyzeVideoJob.
/// Returns job_id for polling and draft_id for later access.
pub async fn start_analysis(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<StartAnalysisRequest>,
) -> ApiResult<Json<StartAnalysisResponse>> {
    // Validate URL using SSRF-safe whitelist validation
    let validated_url = validate_video_url(&request.url)
        .into_result()
        .map_err(ApiError::bad_request)?;

    // Sanitize prompt if provided using unified security function
    let prompt = request.prompt.as_ref().map(|p| sanitize_string(p));

    // Determine TTL based on user's plan
    let ttl_days = get_draft_ttl(&state, &user.uid).await;

    // Generate IDs
    let draft_id = Uuid::new_v4().to_string();

    // Charge credits for analysis (3 credits)
    // This is charged upfront and not refunded if analysis fails
    let credit_context = CreditContext::new(
        CreditOperationType::Analysis,
        "Video analysis",
    ).with_draft_id(&draft_id);

    state
        .user_service
        .check_and_reserve_credits_with_context(&user.uid, ANALYSIS_CREDIT_COST, credit_context)
        .await?;
    let request_id = Uuid::new_v4().to_string();

    // Create the draft record with validated URL
    let mut draft = AnalysisDraft::new(&draft_id, &user.uid, &validated_url, ttl_days)
        .with_request_id(&request_id);

    if let Some(ref p) = prompt {
        draft = draft.with_prompt(p);
    }

    // Store draft in Firestore
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);
    draft_repo.create(&draft).await.map_err(|e| {
        warn!("Failed to create analysis draft: {}", e);
        ApiError::internal("Failed to create analysis draft")
    })?;

    // Create and enqueue the analysis job with validated URL
    let mut job = AnalyzeVideoJob::new(&user.uid, &draft_id, &validated_url);
    if let Some(p) = prompt {
        job = job.with_prompt(p);
    }

    let job_id = job.job_id.to_string();

    state
        .queue
        .enqueue_analyze(job)
        .await
        .map_err(|e| {
            warn!("Failed to enqueue analysis job: {}", e);
            ApiError::internal("Failed to start analysis")
        })?;

    info!(
        "Started analysis job {} for user {} (draft: {})",
        job_id, user.uid, draft_id
    );

    Ok(Json(StartAnalysisResponse { job_id, draft_id }))
}

// ============================================================================
// Poll Analysis Status
// ============================================================================

/// Get the status of an analysis job.
pub async fn get_analysis_status(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<AnalysisStatusResponse>> {
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    let draft = draft_repo
        .get(&draft_id)
        .await
        .map_err(|e| {
            warn!("Failed to get analysis draft: {}", e);
            ApiError::internal("Failed to get analysis status")
        })?
        .ok_or_else(|| ApiError::not_found("Analysis not found"))?;

    // Check if expired
    let status = if draft.is_expired() && !draft.status.is_terminal() {
        AnalysisStatus::Expired
    } else {
        draft.status
    };

    Ok(Json(AnalysisStatusResponse {
        status,
        draft_id: draft.id,
        video_title: draft.video_title,
        error_message: draft.error_message,
        scene_count: draft.scene_count,
        warning_count: draft.warning_count,
    }))
}

// ============================================================================
// List Drafts
// ============================================================================

/// Response for listing drafts.
#[derive(Serialize)]
pub struct ListDraftsResponse {
    pub drafts: Vec<DraftSummary>,
}

/// Summary of a draft for listing.
#[derive(Serialize)]
pub struct DraftSummary {
    pub id: String,
    pub source_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,
    pub status: AnalysisStatus,
    pub scene_count: u32,
    pub created_at: String,
    pub expires_at: String,
}

/// List all analysis drafts for the user.
pub async fn list_drafts(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<ListDraftsResponse>> {
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    let drafts = draft_repo.list(Some(50)).await.map_err(|e| {
        warn!("Failed to list analysis drafts: {}", e);
        ApiError::internal("Failed to list drafts")
    })?;

    let summaries: Vec<DraftSummary> = drafts
        .into_iter()
        .map(|d| {
            let status = if d.is_expired() && !d.status.is_terminal() {
                AnalysisStatus::Expired
            } else {
                d.status
            };
            DraftSummary {
                id: d.id,
                source_url: d.source_url,
                video_title: d.video_title,
                status,
                scene_count: d.scene_count,
                created_at: d.created_at.to_rfc3339(),
                expires_at: d.expires_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(Json(ListDraftsResponse { drafts: summaries }))
}

// ============================================================================
// Get Draft with Scenes
// ============================================================================

/// Response for getting a draft with its scenes.
#[derive(Serialize)]
pub struct DraftWithScenesResponse {
    pub draft: AnalysisDraft,
    pub scenes: Vec<DraftScene>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// Get a draft with all its scenes.
pub async fn get_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<DraftWithScenesResponse>> {
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    let draft = draft_repo
        .get(&draft_id)
        .await
        .map_err(|e| {
            warn!("Failed to get analysis draft: {}", e);
            ApiError::internal("Failed to get draft")
        })?
        .ok_or_else(|| ApiError::not_found("Draft not found"))?;

    // Check if expired
    if draft.is_expired() {
        return Err(ApiError::Conflict(
            "This draft has expired. Please re-analyze the video.".to_string(),
        ));
    }

    // Get scenes
    let mut scenes = draft_repo.get_scenes(&draft_id).await.map_err(|e| {
        warn!("Failed to get draft scenes: {}", e);
        ApiError::internal("Failed to get draft scenes")
    })?;

    // Sort scenes by ID
    scenes.sort_by_key(|s| s.id);

    Ok(Json(DraftWithScenesResponse {
        draft,
        scenes,
        warnings: None, // TODO: Store and return warnings
    }))
}

// ============================================================================
// Delete Draft
// ============================================================================

/// Response for deleting a draft.
#[derive(Serialize)]
pub struct DeleteDraftResponse {
    pub success: bool,
    pub draft_id: String,
}

/// Delete an analysis draft and its scenes.
pub async fn delete_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    user: AuthUser,
) -> ApiResult<Json<DeleteDraftResponse>> {
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    // Verify draft exists and belongs to user
    let draft = draft_repo
        .get(&draft_id)
        .await
        .map_err(|e| {
            warn!("Failed to get analysis draft: {}", e);
            ApiError::internal("Failed to delete draft")
        })?
        .ok_or_else(|| ApiError::not_found("Draft not found"))?;

    if draft.user_id != user.uid {
        return Err(ApiError::not_found("Draft not found"));
    }

    draft_repo.delete(&draft_id).await.map_err(|e| {
        warn!("Failed to delete analysis draft: {}", e);
        ApiError::internal("Failed to delete draft")
    })?;

    info!("Deleted analysis draft {} for user {}", draft_id, user.uid);

    Ok(Json(DeleteDraftResponse {
        success: true,
        draft_id,
    }))
}

// ============================================================================
// Process Draft (Submit for Rendering)
// ============================================================================

/// Response for processing a draft.
#[derive(Serialize)]
pub struct ProcessDraftResponse {
    pub success: bool,
    pub draft_id: String,
    pub video_id: String,
    pub jobs_enqueued: u32,
}

/// Submit selected scenes from a draft for rendering.
///
/// This endpoint is idempotent - duplicate requests with the same
/// idempotency_key will return the same response without creating new jobs.
pub async fn process_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    user: AuthUser,
    Json(request): Json<ProcessDraftRequest>,
) -> ApiResult<Json<ProcessDraftResponse>> {
    // Validate request
    request.validate().map_err(ApiError::bad_request)?;

    if request.analysis_draft_id != draft_id {
        return Err(ApiError::bad_request("Draft ID mismatch"));
    }

    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    // Get draft and verify ownership
    let draft = draft_repo
        .get(&draft_id)
        .await
        .map_err(|e| {
            warn!("Failed to get analysis draft: {}", e);
            ApiError::internal("Failed to process draft")
        })?
        .ok_or_else(|| ApiError::not_found("Draft not found"))?;

    if draft.user_id != user.uid {
        return Err(ApiError::not_found("Draft not found"));
    }

    // Check if draft is ready
    if draft.status != AnalysisStatus::Completed {
        return Err(ApiError::Conflict(format!(
            "Draft is not ready for processing. Status: {:?}",
            draft.status.as_str()
        )));
    }

    // Check expiry
    if draft.is_expired() {
        return Err(ApiError::Conflict(
            "This draft has expired. Please re-analyze the video.".to_string(),
        ));
    }

    // Check idempotency - generate key from user + draft + selection hash
    let idempotency_key = format!(
        "process:{}:{}:{}",
        user.uid,
        draft_id,
        request.idempotency_key()
    );

    // Try to acquire lock (5 minute TTL)
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

    // Get scenes
    let scenes = draft_repo.get_scenes(&draft_id).await.map_err(|e| {
        warn!("Failed to get draft scenes: {}", e);
        ApiError::internal("Failed to get draft scenes")
    })?;

    // Build scene lookup
    let scene_map: std::collections::HashMap<u32, &DraftScene> =
        scenes.iter().map(|s| (s.id, s)).collect();

    // Parse styles
    let full_style: Style = request.full_style.parse().map_err(|_| {
        ApiError::bad_request(format!("Invalid full style: {}", request.full_style))
    })?;
    let split_style: Style = request.split_style.parse().map_err(|_| {
        ApiError::bad_request(format!("Invalid split style: {}", request.split_style))
    })?;

    // Get plan limits for tier validation
    let limits = state.user_service.get_plan_limits(&user.uid).await?;

    // Validate detection tiers are allowed by the user's plan
    for style in [full_style, split_style] {
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

    // Calculate total credits needed
    let mut total_credits = 0u32;
    for selection in &request.selected_scenes {
        if selection.render_full {
            total_credits += full_style.credit_cost();
        }
        if selection.render_split {
            total_credits += split_style.credit_cost();
        }
    }

    // Generate video ID for this processing run
    let video_id = vclip_models::VideoId::new();

    // Build description for credit transaction
    let scene_count = request.selected_scenes.len();
    let styles_used: Vec<&str> = [
        request.selected_scenes.iter().any(|s| s.render_full).then_some(full_style.as_filename_part()),
        request.selected_scenes.iter().any(|s| s.render_split).then_some(split_style.as_filename_part()),
    ].into_iter().flatten().collect();
    let description = format!(
        "Process {} scene{} ({})",
        scene_count,
        if scene_count == 1 { "" } else { "s" },
        styles_used.join(", ")
    );

    // Build metadata for the transaction
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("scene_count".to_string(), scene_count.to_string());
    metadata.insert("styles".to_string(), styles_used.join(","));

    let credit_context = CreditContext::new(
        CreditOperationType::SceneProcessing,
        description,
    )
    .with_video_id(video_id.as_str())
    .with_draft_id(&draft_id)
    .with_metadata(metadata);

    // Check and reserve credits (validates quota + charges upfront)
    state
        .user_service
        .check_and_reserve_credits_with_context(&user.uid, total_credits, credit_context)
        .await?;

    info!(
        "Reserved {} credits for processing draft {} ({} clips)",
        total_credits, draft_id, scene_count
    );

    // Create and enqueue render jobs
    let mut jobs_enqueued = 0u32;

    for selection in &request.selected_scenes {
        let scene = scene_map
            .get(&selection.scene_id)
            .ok_or_else(|| ApiError::bad_request(format!("Scene {} not found", selection.scene_id)))?;

        if selection.render_full {
            let job = RenderSceneStyleJob::new(
                &user.uid,
                video_id.clone(),
                scene.id,
                &scene.title,
                full_style,
                &scene.start,
                &scene.end,
            )
            .with_pad_before(Some(scene.pad_before))
            .with_pad_after(Some(scene.pad_after));

            state
                .queue
                .enqueue_render(job)
                .await
                .map_err(|e| {
                    warn!("Failed to enqueue render job: {}", e);
                    ApiError::internal("Failed to start processing")
                })?;

            jobs_enqueued += 1;
        }

        if selection.render_split {
            let job = RenderSceneStyleJob::new(
                &user.uid,
                video_id.clone(),
                scene.id,
                &scene.title,
                split_style,
                &scene.start,
                &scene.end,
            )
            .with_pad_before(Some(scene.pad_before))
            .with_pad_after(Some(scene.pad_after));

            state
                .queue
                .enqueue_render(job)
                .await
                .map_err(|e| {
                    warn!("Failed to enqueue render job: {}", e);
                    ApiError::internal("Failed to start processing")
                })?;

            jobs_enqueued += 1;
        }
    }

    info!(
        "Enqueued {} render jobs for draft {} (video: {})",
        jobs_enqueued, draft_id, video_id
    );

    Ok(Json(ProcessDraftResponse {
        success: true,
        draft_id,
        video_id: video_id.to_string(),
        jobs_enqueued,
    }))
}

// ============================================================================
// Cost Estimation
// ============================================================================

/// Query parameters for cost estimation.
#[derive(Deserialize)]
pub struct EstimateQuery {
    /// Comma-separated list of scene IDs
    pub scene_ids: String,
    /// Number of FULL renders
    pub full_count: u32,
    /// Number of SPLIT renders
    pub split_count: u32,
    /// Full style name (e.g., "SmartFace", "Cinematic")
    #[serde(default)]
    pub full_style: Option<String>,
    /// Split style name (e.g., "Streamer", "StreamerSplit")
    #[serde(default)]
    pub split_style: Option<String>,
}

/// Get cost and time estimates for processing.
pub async fn estimate_processing(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    Query(query): Query<EstimateQuery>,
    user: AuthUser,
) -> ApiResult<Json<ProcessingEstimate>> {
    let draft_repo = AnalysisDraftRepository::new((*state.firestore).clone(), &user.uid);

    // Get draft
    let draft = draft_repo
        .get(&draft_id)
        .await
        .map_err(|e| {
            warn!("Failed to get analysis draft: {}", e);
            ApiError::internal("Failed to estimate processing")
        })?
        .ok_or_else(|| ApiError::not_found("Draft not found"))?;

    if draft.user_id != user.uid {
        return Err(ApiError::not_found("Draft not found"));
    }

    // Parse scene IDs
    let scene_ids: Vec<u32> = query
        .scene_ids
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // Get scenes to calculate duration
    let scenes = draft_repo.get_scenes(&draft_id).await.map_err(|e| {
        warn!("Failed to get draft scenes: {}", e);
        ApiError::internal("Failed to estimate processing")
    })?;

    let scene_set: std::collections::HashSet<u32> = scene_ids.into_iter().collect();
    let selected_scenes: Vec<&DraftScene> = scenes
        .iter()
        .filter(|s| scene_set.contains(&s.id))
        .collect();

    let scene_count = selected_scenes.len() as u32;
    let total_duration_secs: u32 = selected_scenes.iter().map(|s| s.duration_secs).sum();
    let total_jobs = query.full_count + query.split_count;

    // Calculate credits based on actual style costs
    let full_style_cost = query
        .full_style
        .as_ref()
        .and_then(|s| s.parse::<Style>().ok())
        .map(|s| s.credit_cost())
        .unwrap_or(10); // Default to basic style cost (10 credits)

    let split_style_cost = query
        .split_style
        .as_ref()
        .and_then(|s| s.parse::<Style>().ok())
        .map(|s| s.credit_cost())
        .unwrap_or(10); // Default to streamer style cost (10 credits)

    let estimated_credits =
        (query.full_count * full_style_cost) + (query.split_count * split_style_cost);

    // Estimate time: ~30-60 seconds per job on average
    let estimated_time_min_secs = total_jobs * 30;
    let estimated_time_max_secs = total_jobs * 90;

    // Check quota
    let exceeds_quota = check_exceeds_quota(&state, &user.uid, estimated_credits).await;

    Ok(Json(ProcessingEstimate {
        scene_count,
        total_duration_secs,
        estimated_credits,
        estimated_time_min_secs,
        estimated_time_max_secs,
        full_render_count: query.full_count,
        split_render_count: query.split_count,
        exceeds_quota,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get draft TTL based on user's plan.
async fn get_draft_ttl(state: &AppState, user_id: &str) -> i64 {
    // Check if user has a paid plan
    match state.user_service.has_pro_or_studio_plan(user_id).await {
        Ok(true) => PAID_DRAFT_TTL_DAYS,
        _ => FREE_DRAFT_TTL_DAYS,
    }
}

/// Check if processing would exceed user's quota.
async fn check_exceeds_quota(state: &AppState, user_id: &str, credits_needed: u32) -> bool {
    // Get plan limits
    let limits = match state.user_service.get_plan_limits(user_id).await {
        Ok(l) => l,
        Err(_) => return false, // Default to allowing if we can't check
    };

    // Get current credits usage
    let credits_used = match state.user_service.get_credits_usage(user_id).await {
        Ok(u) => u,
        Err(_) => return false, // Default to allowing if we can't check
    };

    credits_used + credits_needed > limits.monthly_credits_included
}

