//! WebSocket handlers with backpressure support.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info, warn};

use vclip_models::{AspectRatio, CropMode, Style, VideoStatus, WsMessage};
use vclip_firestore::VideoRepository;
use vclip_queue::ProcessVideoJob;

use crate::metrics;
use crate::security::{validate_video_url, sanitize_string, is_valid_video_id, MAX_PROMPT_LENGTH};
use crate::state::AppState;

/// Maximum concurrent WebSocket connections per user.
/// Prevents a single user from consuming too many resources.
const MAX_CONCURRENT_CONNECTIONS_PER_USER: usize = 3;

/// Minimum time between new processing jobs per user (rate limiting).
const MIN_JOB_INTERVAL: Duration = Duration::from_secs(5);

/// Per-user connection tracking for WebSocket rate limiting.
pub struct UserConnectionTracker {
    connections: tokio::sync::RwLock<std::collections::HashMap<String, (usize, Instant)>>,
}

impl UserConnectionTracker {
    pub fn new() -> Self {
        Self {
            connections: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Try to acquire a connection slot for a user.
    /// Returns false if the user has too many concurrent connections or is rate limited.
    pub async fn try_acquire(&self, user_id: &str) -> Result<(), &'static str> {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        if let Some((count, last_job_time)) = connections.get_mut(user_id) {
            // Check rate limit
            if now.duration_since(*last_job_time) < MIN_JOB_INTERVAL {
                return Err("Please wait a few seconds before starting another job");
            }
            // Check concurrent connection limit
            if *count >= MAX_CONCURRENT_CONNECTIONS_PER_USER {
                return Err("Too many concurrent connections. Please wait for existing jobs to complete.");
            }
            *count += 1;
            *last_job_time = now;
        } else {
            connections.insert(user_id.to_string(), (1, now));
        }
        Ok(())
    }

    /// Release a connection slot for a user.
    pub async fn release(&self, user_id: &str) {
        let mut connections = self.connections.write().await;
        if let Some((count, _)) = connections.get_mut(user_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(user_id);
            }
        }
    }
}

impl Default for UserConnectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global user connection tracker.
static USER_CONNECTIONS: std::sync::LazyLock<UserConnectionTracker> = 
    std::sync::LazyLock::new(UserConnectionTracker::new);

/// Styles that require a studio plan (Active Speaker).
const STUDIO_ONLY_STYLES: &[Style] = &[Style::IntelligentSpeaker, Style::IntelligentSplitSpeaker];

/// Styles that require at least a pro plan (Smart Face).
const PRO_ONLY_STYLES: &[Style] = &[Style::Intelligent, Style::IntelligentSplit];

/// Check if any of the styles require a studio plan.
fn contains_studio_only_styles(styles: &[Style]) -> bool {
    styles.iter().any(|s| STUDIO_ONLY_STYLES.contains(s))
}

/// Check if any of the styles require at least a pro plan.
fn contains_pro_only_styles(styles: &[Style]) -> bool {
    styles.iter().any(|s| PRO_ONLY_STYLES.contains(s))
}

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

    // =========================================================================
    // SECURITY: Rate limiting - prevent abuse
    // =========================================================================
    if let Err(rate_limit_msg) = USER_CONNECTIONS.try_acquire(&uid).await {
        warn!(user = %uid, "WebSocket rate limit hit");
        let error = WsMessage::error(rate_limit_msg);
        let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
        drop(tx);
        let _ = send_task.await;
        return;
    }
    
    // Ensure we release the connection slot when done
    let uid_for_cleanup = uid.clone();
    let _guard = scopeguard::guard((), |_| {
        // Release connection slot on scope exit
        tokio::spawn(async move {
            USER_CONNECTIONS.release(&uid_for_cleanup).await;
        });
    });

    // =========================================================================
    // SECURITY: Validate and sanitize all user inputs
    // =========================================================================

    // Validate video URL (SSRF protection)
    let validated_url = match validate_video_url(&request.url) {
        result => match result.into_result() {
            Ok(url) => url,
            Err(e) => {
                let error = WsMessage::error(format!("Invalid video URL: {}", e));
                let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
                drop(tx);
                let _ = send_task.await;
                return;
            }
        }
    };

    // Validate prompt length
    let sanitized_prompt = request.prompt.as_ref().map(|p| {
        if p.len() > MAX_PROMPT_LENGTH {
            warn!(user = %uid, "Prompt truncated from {} to {} chars", p.len(), MAX_PROMPT_LENGTH);
        }
        sanitize_string(p)
    });

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&uid, claims.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", uid, e);
    }

    // =========================================================================
    // QUOTA ENFORCEMENT: Check if user has exceeded their plan limits
    // =========================================================================
    
    // Check monthly clip quota
    let used = match state.user_service.get_monthly_usage(&uid).await {
        Ok(used) => used,
        Err(e) => {
            warn!("Failed to get monthly usage for {}: {}", uid, e);
            let error = WsMessage::error("Unable to verify clip quota. Please try again in a moment.");
            let _ = tx
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    };

    let limits = match state.user_service.get_plan_limits(&uid).await {
        Ok(limits) => limits,
        Err(e) => {
            warn!("Failed to get plan limits for {}: {}", uid, e);
            let error =
                WsMessage::error("Unable to verify your plan limits. Please try again shortly.");
            let _ = tx
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    };

    if used >= limits.max_clips_per_month {
        let error = WsMessage::error(format!(
            "Monthly clip limit exceeded. You've used {} of {} clips this month. Please upgrade your plan or wait until next month.",
            used, limits.max_clips_per_month
        ));
        let _ = tx
            .send(Message::Text(
                serde_json::to_string(&error).unwrap_or_default(),
            ))
            .await;
        drop(tx);
        let _ = send_task.await;
        return;
    }

    // Check storage quota
    let usage = match state.user_service.get_storage_usage(&uid).await {
        Ok(usage) => usage,
        Err(e) => {
            warn!("Failed to get storage usage for {}: {}", uid, e);
            let error =
                WsMessage::error("Unable to verify storage quota. Please try again in a moment.");
            let _ = tx
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            drop(tx);
            let _ = send_task.await;
            return;
        }
    };

    if usage.percentage() >= 100.0 {
        let error = WsMessage::error(format!(
            "Storage limit exceeded. You've used {} of {} storage. Please delete some clips or upgrade your plan.",
            usage.format_total(), usage.format_limit()
        ));
        let _ = tx
            .send(Message::Text(
                serde_json::to_string(&error).unwrap_or_default(),
            ))
            .await;
        drop(tx);
        let _ = send_task.await;
        return;
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

    // Check studio plan requirement for Active Speaker styles
    if contains_studio_only_styles(&styles) {
        match state.user_service.has_studio_plan(&uid).await {
            Ok(false) => {
                let error = WsMessage::error("Active Speaker style is only available for Studio plans. Please upgrade to access this feature.");
                let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                drop(tx);
                let _ = send_task.await;
                return;
            }
            Err(e) => {
                warn!("Failed to check studio plan: {}", e);
            }
            Ok(true) => {}
        }
    }

    // Check pro plan requirement for Smart Face styles
    if contains_pro_only_styles(&styles) {
        match state.user_service.has_pro_or_studio_plan(&uid).await {
            Ok(false) => {
                let error = WsMessage::error("Smart Face style is only available for Pro and Studio plans. Please upgrade to access this feature.");
                let _ = tx.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                drop(tx);
                let _ = send_task.await;
                return;
            }
            Err(e) => {
                warn!("Failed to check pro plan: {}", e);
            }
            Ok(true) => {}
        }
    }

    // Parse crop mode and target aspect
    let crop_mode: CropMode = request.crop_mode.parse().unwrap_or_default();
    let target_aspect: AspectRatio = request.target_aspect.parse().unwrap_or_default();

    // Create job with validated/sanitized parameters
    let job = ProcessVideoJob::new(&uid, &validated_url, styles)
        .with_crop_mode(crop_mode)
        .with_target_aspect(target_aspect)
        .with_custom_prompt(sanitized_prompt);
    let job_id = job.job_id.clone();
    let video_id = job.video_id.clone();

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
    let mut done_sent = false;
    match state.progress.subscribe(&job_id).await {
        Ok(mut stream) => {
            let mut heartbeat = interval(WS_HEARTBEAT_INTERVAL);
            let mut last_activity = std::time::Instant::now();
            let mut credits_consumed = 0u32;

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
                                    WsMessage::ClipUploaded { credits, .. } => {
                                        credits_consumed = credits_consumed.saturating_add(*credits);
                                        "clip_uploaded"
                                    },
                                    WsMessage::ClipProgress { .. } => "clip_progress",
                                    WsMessage::SceneStarted { .. } => "scene_started",
                                    WsMessage::SceneCompleted { .. } => "scene_completed",
                                    WsMessage::Done { .. } => {
                                        done_sent = true;
                                        // Increment usage counter when job completes successfully
                                        if credits_consumed > 0 {
                                            if let Err(e) = state.user_service.increment_usage(&uid, credits_consumed).await {
                                                warn!("Failed to increment usage for user {}: {}", uid, e);
                                            } else {
                                                info!("Incremented usage by {} credits for user {}", credits_consumed, uid);
                                            }
                                        }
                                        "done"
                                    },
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

    // If no done message was emitted (e.g., reconnect after completion), check video status and emit Done if completed.
    if let Err(e) = async {
        if !done_sent {
            let video_repo = VideoRepository::new((*state.firestore).clone(), &uid);
            match video_repo.get(&video_id).await {
                Ok(Some(video)) if video.status == VideoStatus::Completed => {
                    let done = WsMessage::done(video_id.as_str());
                    let _ = tx.send(Message::Text(serde_json::to_string(&done).unwrap())).await;
                }
                Ok(Some(video)) if video.status == VideoStatus::Failed => {
                    let err = WsMessage::error("Video processing failed");
                    let _ = tx.send(Message::Text(serde_json::to_string(&err).unwrap())).await;
                }
                _ => {}
            }
        }
        Ok::<(), ()>(())
    }
    .await
    {
        let _ = e;
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
    
    // SECURITY: Validate video_id format to prevent injection attacks
    if !is_valid_video_id(&request.video_id) {
        let error = WsMessage::error("Invalid video ID format");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
        return;
    }

    // SECURITY: Validate scene_ids count
    if request.scene_ids.is_empty() {
        let error = WsMessage::error("At least one scene ID is required");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
        return;
    }
    if request.scene_ids.len() > 50 {
        let error = WsMessage::error("Cannot reprocess more than 50 scenes at once");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
        return;
    }

    // SECURITY: Validate styles count
    if request.styles.len() > 10 {
        let error = WsMessage::error("Cannot use more than 10 styles");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap_or_default())).await;
        return;
    }

    info!(
        "WebSocket reprocess started for user {}, video {}",
        uid, request.video_id
    );

    // Get or create user
    if let Err(e) = state.user_service.get_or_create_user(&uid, claims.email.as_deref()).await {
        warn!("Failed to get/create user {}: {}", uid, e);
    }

    // =========================================================================
    // QUOTA ENFORCEMENT: Check if user has exceeded their plan limits
    // =========================================================================
    
    // Check monthly clip quota
    let used = match state.user_service.get_monthly_usage(&uid).await {
        Ok(used) => used,
        Err(e) => {
            warn!("Failed to get monthly usage for {}: {}", uid, e);
            let error = WsMessage::error("Unable to verify clip quota. Please try again in a moment.");
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    let limits = match state.user_service.get_plan_limits(&uid).await {
        Ok(limits) => limits,
        Err(e) => {
            warn!("Failed to get plan limits for {}: {}", uid, e);
            let error =
                WsMessage::error("Unable to verify your plan limits. Please try again shortly.");
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    if used >= limits.max_clips_per_month {
        let error = WsMessage::error(format!(
            "Monthly clip limit exceeded. You've used {} of {} clips this month. Please upgrade your plan or wait until next month.",
            used, limits.max_clips_per_month
        ));
        let _ = sender
            .send(Message::Text(
                serde_json::to_string(&error).unwrap_or_default(),
            ))
            .await;
        return;
    }

    // Check storage quota
    let usage = match state.user_service.get_storage_usage(&uid).await {
        Ok(usage) => usage,
        Err(e) => {
            warn!("Failed to get storage usage for {}: {}", uid, e);
            let error =
                WsMessage::error("Unable to verify storage quota. Please try again in a moment.");
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&error).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    if usage.percentage() >= 100.0 {
        let error = WsMessage::error(format!(
            "Storage limit exceeded. You've used {} of {} storage. Please delete some clips or upgrade your plan.",
            usage.format_total(), usage.format_limit()
        ));
        let _ = sender
            .send(Message::Text(
                serde_json::to_string(&error).unwrap_or_default(),
            ))
            .await;
        return;
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

    // Parse styles with "all" expansion support
    let styles = Style::expand_styles(&request.styles);

    if styles.is_empty() {
        let error = WsMessage::error("No valid styles specified");
        let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        return;
    }

    // Check studio plan requirement for Active Speaker styles
    if contains_studio_only_styles(&styles) {
        match state.user_service.has_studio_plan(&uid).await {
            Ok(false) => {
                let error = WsMessage::error("Active Speaker style is only available for Studio plans. Please upgrade to access this feature.");
                let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                return;
            }
            Err(e) => {
                warn!("Failed to check studio plan: {}", e);
            }
            Ok(true) => {}
        }
    }

    // Check pro plan requirement for Smart Face styles
    if contains_pro_only_styles(&styles) {
        match state.user_service.has_pro_or_studio_plan(&uid).await {
            Ok(false) => {
                let error = WsMessage::error("Smart Face style is only available for Pro and Studio plans. Please upgrade to access this feature.");
                let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
                return;
            }
            Err(e) => {
                warn!("Failed to check pro plan: {}", e);
            }
            Ok(true) => {}
        }
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

    // Subscribe to progress events with heartbeat
    let mut done_sent = false;
    match state.progress.subscribe(&job_id).await {
        Ok(mut stream) => {
            let mut heartbeat = interval(WS_HEARTBEAT_INTERVAL);
            let mut last_activity = std::time::Instant::now();
            let mut credits_consumed = 0u32;

            loop {
                tokio::select! {
                    // Progress event from worker
                    event = stream.next() => {
                        match event {
                            Some(event) => {
                                last_activity = std::time::Instant::now();
                                let msg_type = match &event.message {
                                    WsMessage::ClipUploaded { credits, .. } => {
                                        credits_consumed = credits_consumed.saturating_add(*credits);
                                        "clip_uploaded"
                                    }
                                    WsMessage::Done { .. } => {
                                        done_sent = true;
                                        if credits_consumed > 0 {
                                            if let Err(e) = state.user_service.increment_usage(&uid, credits_consumed).await {
                                                warn!("Failed to increment usage for user {}: {}", uid, e);
                                            } else {
                                                info!("Incremented usage by {} credits for user {}", credits_consumed, uid);
                                            }
                                        }
                                        "done"
                                    }
                                    WsMessage::Error { .. } => "error",
                                    WsMessage::Log { .. } => "log",
                                    WsMessage::Progress { .. } => "progress",
                                    WsMessage::ClipProgress { .. } => "clip_progress",
                                    WsMessage::SceneStarted { .. } => "scene_started",
                                    WsMessage::SceneCompleted { .. } => "scene_completed",
                                };

                                metrics::record_ws_message_sent("reprocess", msg_type);

                                let json = match serde_json::to_string(&event.message) {
                                    Ok(j) => j,
                                    Err(_) => continue,
                                };

                                if sender.send(Message::Text(json)).await.is_err() {
                                    warn!("WebSocket send failed, client disconnected");
                                    break;
                                }

                                if matches!(event.message, WsMessage::Done { .. } | WsMessage::Error { .. }) {
                                    break;
                                }
                            }
                            None => break, // Stream ended
                        }
                    }
                    // Heartbeat to keep connection alive during long processing
                    _ = heartbeat.tick() => {
                        if last_activity.elapsed() > WS_HEARTBEAT_INTERVAL / 2 {
                            if sender.send(Message::Ping(vec![])).await.is_err() {
                                warn!("Reprocess heartbeat failed, client disconnected");
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
                                info!("Client closed reprocess connection");
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
            let _ = sender.send(Message::Text(serde_json::to_string(&error).unwrap())).await;
        }
    }

    // If no done message was emitted (e.g., reconnect after completion), check video status and emit Done if completed.
    if !done_sent {
        let video_repo = VideoRepository::new((*state.firestore).clone(), &uid);
        match video_repo.get(&video_id).await {
            Ok(Some(video)) if video.status == VideoStatus::Completed => {
                let done = WsMessage::done(video_id.as_str());
                let _ = sender.send(Message::Text(serde_json::to_string(&done).unwrap())).await;
            }
            Ok(Some(video)) if video.status == VideoStatus::Failed => {
                let err = WsMessage::error("Video processing failed");
                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap())).await;
            }
            _ => {}
        }
    }

    info!("WebSocket reprocess ended for user {}", uid);
}
