//! API middleware.

use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use governor::{Quota, RateLimiter};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn, Span};
use uuid::Uuid;

use crate::metrics;

/// Rate limiter type alias.
pub type GlobalRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Per-IP rate limiter using governor.
pub type IpRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// IP-based rate limiter cache.
#[derive(Clone)]
pub struct RateLimiterCache {
    limiters: Arc<RwLock<HashMap<IpAddr, Arc<IpRateLimiter>>>>,
    quota: Quota,
}

impl RateLimiterCache {
    /// Create a new rate limiter cache.
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(
            NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(10).unwrap()),
        );
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            quota,
        }
    }

    /// Get or create a rate limiter for an IP.
    pub async fn get_limiter(&self, ip: IpAddr) -> Arc<IpRateLimiter> {
        // Try read lock first
        {
            let limiters = self.limiters.read().await;
            if let Some(limiter) = limiters.get(&ip) {
                return Arc::clone(limiter);
            }
        }

        // Need to create new limiter
        let mut limiters = self.limiters.write().await;
        // Double-check after acquiring write lock
        if let Some(limiter) = limiters.get(&ip) {
            return Arc::clone(limiter);
        }

        let limiter = Arc::new(RateLimiter::direct(self.quota));
        limiters.insert(ip, Arc::clone(&limiter));
        limiter
    }

    /// Check rate limit for an IP.
    pub async fn check(&self, ip: IpAddr) -> bool {
        let limiter = self.get_limiter(ip).await;
        limiter.check().is_ok()
    }
}

/// Create a global rate limiter.
pub fn create_rate_limiter(requests_per_second: u32) -> Arc<GlobalRateLimiter> {
    let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(100).unwrap()));
    Arc::new(RateLimiter::direct(quota))
}

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

/// Rate limiting middleware using IP-based rate limiter.
/// This should be applied to routes that need rate limiting.
pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiterCache>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Extract client IP from request
    let ip = extract_client_ip(&request);

    if let Some(ip) = ip {
        if !rate_limiter.check(ip).await {
            warn!(ip = %ip, "Rate limit exceeded");
            metrics::record_rate_limit_hit(request.uri().path());
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("Retry-After", "1")],
                "Rate limit exceeded. Please try again later.",
            )
                .into_response();
        }
    }

    next.run(request).await
}

/// Extract client IP from request headers or connection info.
fn extract_client_ip(request: &Request<Body>) -> Option<IpAddr> {
    // Try X-Forwarded-For header first (for proxied requests)
    if let Some(forwarded) = request.headers().get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // Take the first IP in the chain (original client)
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse() {
                    return Some(ip);
                }
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = request.headers().get("X-Real-IP") {
        if let Ok(ip_str) = real_ip.to_str() {
            if let Ok(ip) = ip_str.parse() {
                return Some(ip);
            }
        }
    }

    // Fall back to connection info (requires ConnectInfo extractor in router)
    request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip())
}
