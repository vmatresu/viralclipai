//! API routes.

use axum::middleware;
use axum::routing::{delete, get};
use axum::Router;

use crate::handlers::{health, ready};
use crate::handlers::videos::{delete_video, get_video_info, list_user_videos};
use crate::middleware::{cors_layer, request_id, request_logging, security_headers};
use crate::state::AppState;
use crate::ws::{ws_process, ws_reprocess};

/// Create the API router.
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // Videos
        .route("/videos/:video_id", get(get_video_info))
        .route("/videos/:video_id", delete(delete_video))
        .route("/user/videos", get(list_user_videos));

    let ws_routes = Router::new()
        .route("/ws/process", get(ws_process))
        .route("/ws/reprocess", get(ws_reprocess));

    let health_routes = Router::new()
        .route("/health", get(health))
        .route("/healthz", get(health))
        .route("/ready", get(ready));

    Router::new()
        .nest("/api", api_routes)
        .merge(ws_routes)
        .merge(health_routes)
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(request_id))
        .layer(middleware::from_fn(request_logging))
        .layer(cors_layer(&state.config.cors_origins))
        .with_state(state)
}
