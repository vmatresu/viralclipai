//! Firestore metrics collection.
//!
//! Provides standardized metrics for monitoring Firestore operations:
//! - Request counters by operation and status
//! - Latency histograms
//! - Retry counters

use metrics::{counter, histogram};

// =============================================================================
// Metric Names
// =============================================================================

/// Metric name constants for consistency.
pub mod names {
    /// Total Firestore requests by operation and status.
    pub const REQUESTS_TOTAL: &str = "firestore_requests_total";

    /// Total retry attempts by operation.
    pub const RETRIES_TOTAL: &str = "firestore_retries_total";

    /// Request latency in seconds by operation.
    pub const LATENCY_SECONDS: &str = "firestore_latency_seconds";
}

// =============================================================================
// Recording Functions
// =============================================================================

/// Record metrics for a completed Firestore request.
pub fn record_request(operation: &str, status: u16, latency_ms: f64) {
    let status_str = status.to_string();

    counter!(
        names::REQUESTS_TOTAL,
        "operation" => operation.to_string(),
        "status" => status_str
    )
    .increment(1);

    histogram!(
        names::LATENCY_SECONDS,
        "operation" => operation.to_string()
    )
    .record(latency_ms / 1000.0);
}

/// Record a retry attempt.
pub fn record_retry(operation: &str) {
    counter!(
        names::RETRIES_TOTAL,
        "operation" => operation.to_string()
    )
    .increment(1);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_names() {
        assert!(names::REQUESTS_TOTAL.contains("requests"));
        assert!(names::RETRIES_TOTAL.contains("retries"));
        assert!(names::LATENCY_SECONDS.contains("latency"));
    }
}
