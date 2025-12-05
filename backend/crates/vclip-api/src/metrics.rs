//! Prometheus metrics for the API server.

use axum::body::Body;
use axum::http::{Request, Response};
use axum::middleware::Next;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;

/// Initialize the Prometheus metrics recorder.
/// Returns a handle that can be used to render metrics.
pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

/// Metric names as constants for consistency.
pub mod names {
    // HTTP metrics
    pub const HTTP_REQUESTS_TOTAL: &str = "vclip_http_requests_total";
    pub const HTTP_REQUEST_DURATION_SECONDS: &str = "vclip_http_request_duration_seconds";
    pub const HTTP_REQUESTS_IN_FLIGHT: &str = "vclip_http_requests_in_flight";

    // WebSocket metrics
    pub const WS_CONNECTIONS_TOTAL: &str = "vclip_ws_connections_total";
    pub const WS_CONNECTIONS_ACTIVE: &str = "vclip_ws_connections_active";
    pub const WS_MESSAGES_SENT: &str = "vclip_ws_messages_sent_total";
    pub const WS_MESSAGES_RECEIVED: &str = "vclip_ws_messages_received_total";

    // Queue metrics
    pub const QUEUE_LENGTH: &str = "vclip_queue_length";
    pub const QUEUE_DLQ_LENGTH: &str = "vclip_queue_dlq_length";
    pub const JOBS_ENQUEUED_TOTAL: &str = "vclip_jobs_enqueued_total";
    pub const JOBS_COMPLETED_TOTAL: &str = "vclip_jobs_completed_total";
    pub const JOBS_FAILED_TOTAL: &str = "vclip_jobs_failed_total";

    // Processing metrics
    pub const FFMPEG_DURATION_SECONDS: &str = "vclip_ffmpeg_duration_seconds";
    pub const CLIPS_PROCESSED_TOTAL: &str = "vclip_clips_processed_total";
    pub const DOWNLOAD_DURATION_SECONDS: &str = "vclip_download_duration_seconds";
    pub const UPLOAD_DURATION_SECONDS: &str = "vclip_upload_duration_seconds";

    // Rate limiting metrics
    pub const RATE_LIMIT_HITS_TOTAL: &str = "vclip_rate_limit_hits_total";
}

/// Record an HTTP request.
pub fn record_http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    let labels = [
        ("method", method.to_string()),
        ("path", sanitize_path(path)),
        ("status", status.to_string()),
    ];

    counter!(names::HTTP_REQUESTS_TOTAL, &labels).increment(1);
    histogram!(names::HTTP_REQUEST_DURATION_SECONDS, &labels).record(duration_secs);
}

/// Record WebSocket connection.
pub fn record_ws_connection(endpoint: &str) {
    let labels = [("endpoint", endpoint.to_string())];
    counter!(names::WS_CONNECTIONS_TOTAL, &labels).increment(1);
}

/// Update active WebSocket connections gauge.
pub fn set_ws_active_connections(count: i64) {
    gauge!(names::WS_CONNECTIONS_ACTIVE).set(count as f64);
}

/// Record WebSocket message sent.
pub fn record_ws_message_sent(endpoint: &str, message_type: &str) {
    let labels = [
        ("endpoint", endpoint.to_string()),
        ("type", message_type.to_string()),
    ];
    counter!(names::WS_MESSAGES_SENT, &labels).increment(1);
}

/// Record WebSocket message received.
pub fn record_ws_message_received(endpoint: &str) {
    let labels = [("endpoint", endpoint.to_string())];
    counter!(names::WS_MESSAGES_RECEIVED, &labels).increment(1);
}

/// Update queue length gauge.
pub fn set_queue_length(length: u64) {
    gauge!(names::QUEUE_LENGTH).set(length as f64);
}

/// Update DLQ length gauge.
pub fn set_dlq_length(length: u64) {
    gauge!(names::QUEUE_DLQ_LENGTH).set(length as f64);
}

/// Record job enqueued.
pub fn record_job_enqueued(job_type: &str) {
    let labels = [("type", job_type.to_string())];
    counter!(names::JOBS_ENQUEUED_TOTAL, &labels).increment(1);
}

/// Record job completed.
pub fn record_job_completed(job_type: &str) {
    let labels = [("type", job_type.to_string())];
    counter!(names::JOBS_COMPLETED_TOTAL, &labels).increment(1);
}

/// Record job failed.
pub fn record_job_failed(job_type: &str) {
    let labels = [("type", job_type.to_string())];
    counter!(names::JOBS_FAILED_TOTAL, &labels).increment(1);
}

/// Record FFmpeg processing duration.
pub fn record_ffmpeg_duration(style: &str, duration_secs: f64) {
    let labels = [("style", style.to_string())];
    histogram!(names::FFMPEG_DURATION_SECONDS, &labels).record(duration_secs);
}

/// Record clip processed.
pub fn record_clip_processed(style: &str) {
    let labels = [("style", style.to_string())];
    counter!(names::CLIPS_PROCESSED_TOTAL, &labels).increment(1);
}

/// Record download duration.
pub fn record_download_duration(duration_secs: f64) {
    histogram!(names::DOWNLOAD_DURATION_SECONDS).record(duration_secs);
}

/// Record upload duration.
pub fn record_upload_duration(duration_secs: f64) {
    histogram!(names::UPLOAD_DURATION_SECONDS).record(duration_secs);
}

/// Record rate limit hit.
pub fn record_rate_limit_hit(endpoint: &str) {
    let labels = [("endpoint", endpoint.to_string())];
    counter!(names::RATE_LIMIT_HITS_TOTAL, &labels).increment(1);
}

/// Sanitize path for metrics labels (remove IDs, etc.).
fn sanitize_path(path: &str) -> String {
    // Replace UUIDs and numeric IDs with placeholders
    let path = regex_lite::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
        .unwrap()
        .replace_all(path, ":id");
    let path = regex_lite::Regex::new(r"/[0-9]+(/|$)")
        .unwrap()
        .replace_all(&path, "/:id$1");
    // Normalize video IDs (alphanumeric strings after /videos/)
    let path = regex_lite::Regex::new(r"/videos/[a-zA-Z0-9_-]+")
        .unwrap()
        .replace_all(&path, "/videos/:video_id");
    // Normalize clip names
    let path = regex_lite::Regex::new(r"/clips/[a-zA-Z0-9_.-]+")
        .unwrap()
        .replace_all(&path, "/clips/:clip_name");
    path.to_string()
}

/// Metrics middleware for HTTP requests.
pub async fn metrics_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    // Increment in-flight counter
    gauge!(names::HTTP_REQUESTS_IN_FLIGHT).increment(1.0);

    let response = next.run(request).await;

    // Decrement in-flight counter
    gauge!(names::HTTP_REQUESTS_IN_FLIGHT).decrement(1.0);

    let status = response.status().as_u16();
    let duration = start.elapsed().as_secs_f64();

    record_http_request(&method, &path, status, duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        assert_eq!(
            sanitize_path("/api/videos/abc123-def456/clips/clip_01.mp4"),
            "/api/videos/:video_id/clips/:clip_name"
        );
        assert_eq!(
            sanitize_path("/api/videos/550e8400-e29b-41d4-a716-446655440000"),
            "/api/videos/:id"
        );
    }
}
