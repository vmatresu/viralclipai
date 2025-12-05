//! WebSocket handlers with backpressure support.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info, warn};

use vclip_models::{AspectRatio, CropMode, Style, VideoStatus, WsMessage};
use vclip_queue::ProcessVideoJob;

use crate::metrics;
use crate::state::AppState;

/// Global counter for active WebSocket connections.
static ACTIVE_WS_CONNECTIONS: AtomicI64 = AtomicI64::new(0);

/// Configuration for WebSocket backpressure.
const WS_SEND_BUFFER_SIZE: usize = 32;
const WS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const WS_CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

/// Send a WebSocket message with backpressure handling.
async fn send_ws_message(tx: &mpsc::Sender<Message>, msg: WsMessage) -> bool {
    let json = match serde_json::to_string(&msg) {
        Ok(j) => j,
        Err(_) => return false,
    };
    // Use try_send for non-blocking, fall back to blocking send
    match tx.try_send(Message::Text(json.clone())) {
        Ok(_) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            // Channel full - apply backpressure by blocking
            debug!("WebSocket send buffer full, applying backpressure");
            tx.send(Message::Text(json)).await.is_ok()
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

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
    // Track connection
    let count = ACTIVE_WS_CONNECTIONS.fetch_add(1, Ordering::SeqCst) + 1;
    metrics::set_ws_active_connections(count);
    metrics::record_ws_connection("process");

    ws.on_upgrade(|socket| async move {
        handle_process_socket(socket, state).await;
        // Decrement on disconnect
        let count = ACTIVE_WS_CONNECTIONS.fetch_sub(1, Ordering::SeqCst) - 1;
        metrics::set_ws_active_connections(count);
    })
}

/// Handle process WebSocket connection with backpressure.
async fn handle_process_socket(socket: WebSocket, state: AppState) {
    let (ws_sender, mut receiver) = socket.split();

    // Create a bounded channel for backpressure
    let (tx, mut rx) = mpsc::channel::<Message>(WS_SEND_BUFFER_SIZE);

    // Spawn a task to handle sending messages with backpressure
    let send_task = tokio::spawn(async move {
        let mut ws_sender = ws_sender;
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
        ws_sender
    });

    // Wait for initial request message with timeout
    let request: WsProcessRequest = match tokio::time::timeout(
        WS_CLIENT_TIMEOUT,
        receiver.next(),
    )
    .await
    {
        Ok(Some(Ok(Message::Text(text)))) => {
            metrics::record_ws_message_received("process");
            match serde_json::from_str(&text) {
                Ok(req) => req,
                Err(e) => {
                    let error = WsMessage::error(format!("Invalid request: {}", e));
                    let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                    drop(tx);
                    let _ = send_task.await;
                    return;
                }
            }
        }
        Ok(_) | Err(_) => {
            let error = WsMessage::error("Expected JSON message or connection timeout");
            let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    };

    // Verify token
    let claims = match state.jwks.verify_token(&request.token).await {
        Ok(c) => c,
        Err(e) => {
            let error = WsMessage::error(format!("Authentication failed: {}", e));
            let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    };

    let uid = claims.uid().to_string();
    info!("WebSocket process started for user {}", uid);

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&uid, claims.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", uid, e);
    }

    // Parse styles with "all" expansion support
    let style_strs = request.styles.unwrap_or_else(|| vec!["split".to_string()]);
    let styles = Style::expand_styles(&style_strs);

    if styles.is_empty() {
        let error = WsMessage::error("No valid styles specified");
        let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        drop(tx);
        let _ = send_task.await;
        return;
    }

    // Parse crop mode and target aspect
    let crop_mode: CropMode = request.crop_mode.parse().unwrap_or_default();
    let target_aspect: AspectRatio = request.target_aspect.parse().unwrap_or_default();

    // Create job with all parameters
    let job = ProcessVideoJob::new(&uid, &request.url, styles)
        .with_crop_mode(crop_mode)
        .with_target_aspect(target_aspect)
        .with_custom_prompt(request.prompt.clone());
    let job_id = job.job_id.clone();
    let _video_id = job.video_id.clone();

    // Enqueue job
    match state.queue.enqueue_process(job).await {
        Ok(_) => {
            metrics::record_job_enqueued("process_video");
            let log = WsMessage::log("Job enqueued, processing will begin shortly...");
            send_ws_message(&tx, log).await;
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to enqueue job: {}", e));
            let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    }

    // Subscribe to progress events with heartbeat
    match state.progress.subscribe(&job_id).await {
        Ok(mut stream) => {
            let mut heartbeat = interval(WS_HEARTBEAT_INTERVAL);
            let mut last_activity = std::time::Instant::now();

            loop {
                tokio::select! {
                    // Progress event from worker
                    event = stream.next() => {
                        match event {
                            Some(event) => {
                                last_activity = std::time::Instant::now();
                                let msg_type = match &event.message {
                                    WsMessage::Log { .. } => "log",
                                    WsMessage::Progress { .. } => "progress",
                                    WsMessage::ClipUploaded { .. } => "clip_uploaded",
                                    WsMessage::Done { .. } => "done",
                                    WsMessage::Error { .. } => "error",
                                };
                                metrics::record_ws_message_sent("process", msg_type);

                                if !send_ws_message(&tx, event.message.clone()).await {
                                    warn!("WebSocket send failed, client disconnected");
                                    break;
                                }

                                // Check for completion
                                if matches!(event.message, WsMessage::Done { .. } | WsMessage::Error { .. }) {
                                    break;
                                }
                            }
                            None => break, // Stream ended
                        }
                    }
                    // Heartbeat to keep connection alive
                    _ = heartbeat.tick() => {
                        // Send ping if no recent activity
                        if last_activity.elapsed() > WS_HEARTBEAT_INTERVAL / 2 {
                            if tx.send(Message::Ping(vec![])).await.is_err() {
                                warn!("Heartbeat failed, client disconnected");
                                break;
                            }
                        }
                    }
                    // Client message (for pong responses)
                    client_msg = receiver.next() => {
                        match client_msg {
                            Some(Ok(Message::Pong(_))) => {
                                last_activity = std::time::Instant::now();
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                info!("Client closed connection");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Err(e) => {
            let error = WsMessage::error(format!("Failed to subscribe to progress: {}", e));
            let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        }
    }

    // Clean up
    drop(tx);
    let _ = send_task.await;
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

    // Parse styles with "all" expansion support
    let styles = Style::expand_styles(&request.styles);

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

    // Parse crop mode and target aspect
    let crop_mode: CropMode = request.crop_mode.parse().unwrap_or_default();
    let target_aspect: AspectRatio = request.target_aspect.parse().unwrap_or_default();

    // Create job with all parameters
    let video_id = vclip_models::VideoId::from_string(&request.video_id);
    let job = vclip_queue::ReprocessScenesJob::new(&uid, video_id.clone(), request.scene_ids, styles)
        .with_crop_mode(crop_mode)
        .with_target_aspect(target_aspect);
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
