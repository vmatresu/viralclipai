//! API middleware.

use axum::body::Body;
use axum::http::{HeaderValue, Request, Response};
use axum::middleware::Next;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Span};
use uuid::Uuid;

/// Create CORS layer.
pub fn cors_layer(origins: &[String]) -> CorsLayer {
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
        .max_age(std::time::Duration::from_secs(600));

    if origins.iter().any(|o| o == "*") {
        cors.allow_origin(Any)
    } else {
        let origins: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        cors.allow_origin(origins)
    }
}

/// Security headers middleware.
pub async fn security_headers(
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    headers.insert(
        "Permissions-Policy",
        "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()"
            .parse()
            .unwrap(),
    );

    response
}

/// Request ID middleware.
pub async fn request_id(
    mut request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Generate or extract request ID
    let request_id = request
        .headers()
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Add to request extensions
    request.extensions_mut().insert(request_id.clone());

    // Record in span
    Span::current().record("request_id", &request_id);

    let mut response = next.run(request).await;

    // Add to response headers
    response
        .headers_mut()
        .insert("X-Request-ID", request_id.parse().unwrap());

    response
}

/// Request logging middleware.
pub async fn request_logging(
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status();
    let duration = start.elapsed();

    // Skip health check logging
    if uri.path() != "/health" && uri.path() != "/healthz" && uri.path() != "/ready" {
        info!(
            method = %method,
            uri = %uri,
            status = %status,
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    }

    response
}
