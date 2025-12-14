//! API routes.

use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use tower_http::limit::RequestBodyLimitLayer;

use crate::handlers::{health, ready};
use crate::handlers::admin::{
    enqueue_synthetic_job, get_queue_status, get_system_info,
    get_user, list_users, recalculate_user_storage, reset_video_status,
    update_user_plan, update_user_usage,
};
use crate::handlers::analysis::{
    delete_draft, estimate_processing, get_analysis_status, get_draft,
    list_drafts, process_draft, start_analysis,
};
use crate::handlers::clip_delivery::{
    create_share, get_download_url, get_playback_url, get_thumbnail_url,
    resolve_share, revoke_share,
};
use crate::handlers::settings::{get_settings, update_settings};
use crate::handlers::storage::{check_storage_quota, get_storage_quota};
use crate::handlers::videos::{
    bulk_delete_clips, bulk_delete_videos, delete_all_clips, delete_clip, delete_video, get_video_highlights, get_video_info,
    get_video_scene_styles, list_user_videos, get_processing_status, reprocess_scenes, stream_clip, update_video_title,
};
use crate::metrics::metrics_middleware;
use crate::middleware::{cors_layer, rate_limit_middleware, request_id, request_logging, security_headers, RateLimiterCache};
use crate::state::AppState;
use crate::ws::{ws_process, ws_reprocess};

/// Create the API router.
pub fn create_router(state: AppState, metrics_handle: Option<PrometheusHandle>) -> Router {
    // Analysis workflow routes (new two-step flow)
    let analysis_routes = Router::new()
        // Start analysis
        .route("/analyze", post(start_analysis))
        // Poll analysis status
        .route("/analyze/:draft_id/status", get(get_analysis_status))
        // Draft management
        .route("/drafts", get(list_drafts))
        .route("/drafts/:draft_id", get(get_draft))
        .route("/drafts/:draft_id", delete(delete_draft))
        // Process draft (submit for rendering)
        .route("/drafts/:draft_id/process", post(process_draft))
        // Cost estimation
        .route("/drafts/:draft_id/estimate", get(estimate_processing));

    let video_routes = Router::new()
        // Single video operations
        .route("/videos/:video_id", get(get_video_info))
        .route("/videos/:video_id", delete(delete_video))
        // Bulk delete
        .route("/videos", delete(bulk_delete_videos))
        // Clip operations
        .route("/videos/:video_id/clips/:clip_name", get(stream_clip))
        .route("/videos/:video_id/clips/:clip_name", delete(delete_clip))
        // Bulk clip operations
        .route("/videos/:video_id/clips", delete(bulk_delete_clips))
        .route("/videos/:video_id/clips/all", delete(delete_all_clips))
        // Highlights
        .route("/videos/:video_id/highlights", get(get_video_highlights))
        // Scene/style index for overwrite detection
        .route("/videos/:video_id/scene-styles", get(get_video_scene_styles))
        // Title update
        .route("/videos/:video_id/title", patch(update_video_title))
        // Reprocess
        .route("/videos/:video_id/reprocess", post(reprocess_scenes))
        // User videos list
        .route("/user/videos", get(list_user_videos))
        .route("/user/videos/processing-status", get(get_processing_status));

    // Clip delivery routes (secure playback/download/share URLs)
    let clip_routes = Router::new()
        // Playback URL (short-lived presigned URL for video player)
        .route("/clips/:clip_id/play-url", post(get_playback_url))
        // Download URL (short-lived presigned URL with Content-Disposition)
        .route("/clips/:clip_id/download-url", post(get_download_url))
        // Thumbnail URL
        .route("/clips/:clip_id/thumbnail-url", post(get_thumbnail_url))
        // Share management
        .route("/clips/:clip_id/share", post(create_share))
        .route("/clips/:clip_id/share", delete(revoke_share));

    let settings_routes = Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", post(update_settings));

    let storage_routes = Router::new()
        .route("/storage/quota", get(get_storage_quota))
        .route("/storage/check", post(check_storage_quota));

    // Admin routes for canary testing and user management (superadmin only)
    let admin_routes = Router::new()
        .route("/admin/jobs/synthetic", post(enqueue_synthetic_job))
        .route("/admin/queue/status", get(get_queue_status))
        .route("/admin/system/info", get(get_system_info))
        // User management
        .route("/admin/users", get(list_users))
        .route("/admin/users/:uid", get(get_user))
        .route("/admin/users/:uid/plan", patch(update_user_plan))
        .route("/admin/users/:uid/usage", patch(update_user_usage))
        // Storage management
        .route("/admin/users/:uid/storage/recalculate", post(recalculate_user_storage))
        // Video recovery (for stuck jobs)
        .route("/admin/users/:uid/videos/:video_id/reset", post(reset_video_status));

    // Create rate limiter for API routes
    let rate_limiter = std::sync::Arc::new(RateLimiterCache::new(state.config.rate_limit_rps));

    // Create a more restrictive rate limiter for public share routes (5 req/sec)
    // This helps prevent brute-force attacks on share slugs
    let share_rate_limiter = std::sync::Arc::new(RateLimiterCache::new(5));

    let api_routes = Router::new()
        .merge(analysis_routes)
        .merge(video_routes)
        .merge(clip_routes)
        .merge(settings_routes)
        .merge(storage_routes)
        .merge(admin_routes)
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit_middleware,
        ));

    // Public share resolution route (no auth required, but rate-limited)
    let share_routes = Router::new()
        .route("/c/:share_slug", get(resolve_share))
        .layer(middleware::from_fn_with_state(
            share_rate_limiter,
            rate_limit_middleware,
        ));

    let ws_routes = Router::new()
        .route("/ws/process", get(ws_process))
        .route("/ws/reprocess", get(ws_reprocess));

    let health_routes = Router::new()
        .route("/health", get(health))
        .route("/healthz", get(health))
        .route("/ready", get(ready));

    // Metrics endpoint (if enabled)
    let metrics_routes = if let Some(handle) = metrics_handle {
        Router::new().route("/metrics", get(move || async move { handle.render() }))
    } else {
        Router::new()
    };

    Router::new()
        .nest("/api", api_routes)
        .merge(share_routes) // Public /c/{share_slug} route
        .merge(ws_routes)
        .merge(health_routes)
        .merge(metrics_routes)
        // SECURITY: Request body size limit to prevent DoS attacks
        .layer(RequestBodyLimitLayer::new(state.config.max_body_size))
        .layer(middleware::from_fn(metrics_middleware))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(request_id))
        .layer(middleware::from_fn(request_logging))
        .layer(cors_layer(&state.config.cors_origins))
        .with_state(state)
}
