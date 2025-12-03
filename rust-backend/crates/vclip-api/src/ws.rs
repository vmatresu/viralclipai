//! WebSocket handlers.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tracing::{info, warn};

use vclip_models::{Style, VideoStatus, WsMessage};
use vclip_queue::ProcessVideoJob;

use crate::state::AppState;

/// WebSocket process request.
#[derive(Debug, Deserialize)]
pub struct WsProcessRequest {
    pub token: String,
    pub url: String,
    #[serde(default)]
    pub styles: Option<Vec<String>>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,
    #[serde(default = "default_aspect")]
    pub target_aspect: String,
}

fn default_crop_mode() -> String {
    "none".to_string()
}

fn default_aspect() -> String {
    "9:16".to_string()
}

/// WebSocket reprocess request.
#[derive(Debug, Deserialize)]
pub struct WsReprocessRequest {
    pub token: String,
    pub video_id: String,
    pub scene_ids: Vec<u32>,
    pub styles: Vec<String>,
    #[serde(default = "default_crop_mode")]
    pub crop_mode: String,
    #[serde(default = "default_aspect")]
    pub target_aspect: String,
}

/// WebSocket process endpoint.
pub async fn ws_process(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_process_socket(socket, state))
}

/// Handle process WebSocket connection.
async fn handle_process_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Wait for initial request message
    let request: WsProcessRequest = match receiver.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
            Ok(req) => req,
            Err(e) => {
                let error = WsMessage::error(format!("Invalid request: {}", e));
                let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                return;
            }
        },
        _ => {
            let error = WsMessage::error("Expected JSON message");
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    };

    // Verify token
    let claims = match state.jwks.verify_token(&request.token).await {
        Ok(c) => c,
        Err(e) => {
            let error = WsMessage::error(format!("Authentication failed: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    };

    let uid = claims.uid().to_string();
    info!("WebSocket process started for user {}", uid);

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&uid, claims.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", uid, e);
    }

    // Parse styles
    let styles: Vec<Style> = request
        .styles
        .unwrap_or_else(|| vec!["split".to_string()])
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if styles.is_empty() {
        let error = WsMessage::error("No valid styles specified");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        return;
    }

    // Create job
    let job = ProcessVideoJob::new(&uid, &request.url, styles);
    let job_id = job.job_id.clone();
    let _video_id = job.video_id.clone();

    // Enqueue job
    match state.queue.enqueue_process(job).await {
        Ok(_) => {
            let log = WsMessage::log("Job enqueued, processing will begin shortly...");
            let _ = sender.send(Message::Text(serde_json::to_string(&log).unwrap())).await;
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to enqueue job: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    }

    // Subscribe to progress events
    match state.progress.subscribe(&job_id).await {
        Ok(mut stream) => {
            while let Some(event) = stream.next().await {
                let json = match serde_json::to_string(&event.message) {
                    Ok(j) => j,
                    Err(_) => continue,
                };

                if sender.send(Message::Text(json)).await.is_err() {
                    warn!("WebSocket send failed, client disconnected");
                    break;
                }

                // Check for completion
                if matches!(event.message, WsMessage::Done { .. } | WsMessage::Error { .. }) {
                    break;
                }
            }
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to subscribe to progress: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        }
    }

    info!("WebSocket process ended for user {}", uid);
}

/// WebSocket reprocess endpoint.
pub async fn ws_reprocess(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_reprocess_socket(socket, state))
}

/// Handle reprocess WebSocket connection.
async fn handle_reprocess_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Wait for initial request message
    let request: WsReprocessRequest = match receiver.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
            Ok(req) => req,
            Err(e) => {
                let error = WsMessage::error(format!("Invalid request: {}", e));
                let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                return;
            }
        },
        _ => {
            let error = WsMessage::error("Expected JSON message");
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    };

    // Verify token
    let claims = match state.jwks.verify_token(&request.token).await {
        Ok(c) => c,
        Err(e) => {
            let error = WsMessage::error(format!("Authentication failed: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    };

    let uid = claims.uid().to_string();
    info!(
        "WebSocket reprocess started for user {}, video {}",
        uid, request.video_id
    );

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&uid, claims.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", uid, e);
    }

    // Check ownership
    match state.user_service.user_owns_video(&uid, &request.video_id).await {
        Ok(false) => {
            let error = WsMessage::error("Video not found or access denied");
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to verify ownership: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
        Ok(true) => {}
    }

    // Check if video is currently processing
    match state.user_service.is_video_processing(&uid, &request.video_id).await {
        Ok(true) => {
            let error = WsMessage::error("Video is currently processing. Please wait for it to complete before reprocessing.");
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
        Err(e) => {
            warn!("Failed to check processing status: {}", e);
        }
        Ok(false) => {}
    }

    // Check plan restrictions (pro/enterprise only)
    match state.user_service.has_pro_or_enterprise_plan(&uid).await {
        Ok(false) => {
            let error = WsMessage::error("Scene reprocessing is only available for Pro and Enterprise plans. Please upgrade to access this feature.");
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
        Err(e) => {
            warn!("Failed to check plan: {}", e);
        }
        Ok(true) => {}
    }

    // Parse styles
    let styles: Vec<Style> = request
        .styles
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if styles.is_empty() {
        let error = WsMessage::error("No valid styles specified");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        return;
    }

    // Validate plan limits
    let total_clips = request.scene_ids.len() as u32 * styles.len() as u32;
    if let Err(e) = state.user_service.validate_plan_limits(&uid, total_clips).await {
        let error = WsMessage::error(format!("{}", e));
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        return;
    }

    // Update video status to processing
    if let Err(e) = state.user_service.update_video_status(&uid, &request.video_id, VideoStatus::Processing).await {
        warn!("Failed to update video status: {}", e);
    }

    // Create job
    let video_id = vclip_models::VideoId::from_string(&request.video_id);
    let job = vclip_queue::ReprocessScenesJob::new(&uid, video_id.clone(), request.scene_ids, styles);
    let job_id = job.job_id.clone();

    // Enqueue job
    match state.queue.enqueue_reprocess(job).await {
        Ok(_) => {
            let log = WsMessage::log("Reprocess job enqueued...");
            let _ = sender.send(Message::Text(serde_json::to_string(&log).unwrap())).await;
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to enqueue job: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            return;
        }
    }

    // Subscribe to progress events
    match state.progress.subscribe(&job_id).await {
        Ok(mut stream) => {
            while let Some(event) = stream.next().await {
                let json = match serde_json::to_string(&event.message) {
                    Ok(j) => j,
                    Err(_) => continue,
                };

                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }

                if matches!(event.message, WsMessage::Done { .. } | WsMessage::Error { .. }) {
                    break;
                }
            }
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to subscribe to progress: {}", e));
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        }
    }

    info!("WebSocket reprocess ended for user {}", uid);
}
