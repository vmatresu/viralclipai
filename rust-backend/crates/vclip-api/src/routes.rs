//! API routes.

use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;

use crate::handlers::{health, ready};
use crate::handlers::admin::{enqueue_synthetic_job, get_queue_status, get_system_info};
use crate::handlers::settings::{get_settings, update_settings};
use crate::handlers::videos::{
    bulk_delete_videos, delete_clip, delete_video, get_video_highlights, get_video_info,
    list_user_videos, reprocess_scenes, stream_clip, update_video_title,
};
use crate::metrics::metrics_middleware;
use crate::middleware::{cors_layer, rate_limit_middleware, request_id, request_logging, security_headers, RateLimiterCache};
use crate::state::AppState;
use crate::ws::{ws_process, ws_reprocess};

/// Create the API router.
pub fn create_router(state: AppState, metrics_handle: Option<PrometheusHandle>) -> Router {
    let video_routes = Router::new()
        // Single video operations
        .route("/videos/:video_id", get(get_video_info))
        .route("/videos/:video_id", delete(delete_video))
        // Bulk delete
        .route("/videos", delete(bulk_delete_videos))
        // Clip operations
        .route("/videos/:video_id/clips/:clip_name", get(stream_clip))
        .route("/videos/:video_id/clips/:clip_name", delete(delete_clip))
        // Highlights
        .route("/videos/:video_id/highlights", get(get_video_highlights))
        // Title update
        .route("/videos/:video_id/title", patch(update_video_title))
        // Reprocess
        .route("/videos/:video_id/reprocess", post(reprocess_scenes))
        // User videos list
        .route("/user/videos", get(list_user_videos));

    let settings_routes = Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", post(update_settings));

    // Admin routes for canary testing (superadmin only)
    let admin_routes = Router::new()
        .route("/admin/jobs/synthetic", post(enqueue_synthetic_job))
        .route("/admin/queue/status", get(get_queue_status))
        .route("/admin/system/info", get(get_system_info));

    // Create rate limiter for API routes
    let rate_limiter = std::sync::Arc::new(RateLimiterCache::new(state.config.rate_limit_rps));

    let api_routes = Router::new()
        .merge(video_routes)
        .merge(settings_routes)
        .merge(admin_routes)
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
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
        .merge(ws_routes)
        .merge(health_routes)
        .merge(metrics_routes)
        .layer(middleware::from_fn(metrics_middleware))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(request_id))
        .layer(middleware::from_fn(request_logging))
        .layer(cors_layer(&state.config.cors_origins))
        .with_state(state)
}
