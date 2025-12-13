//! Tests for Firestore client functionality.

use std::time::Duration;

use serial_test::serial;

use crate::client::FirestoreConfig;
use crate::error::FirestoreError;
use crate::retry::RetryConfig;

// =============================================================================
// Test Helpers
// =============================================================================

#[allow(dead_code)]
fn test_config() -> FirestoreConfig {
    FirestoreConfig {
        project_id: "test-project".to_string(),
        database_id: "(default)".to_string(),
        timeout: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(2),
        retry: RetryConfig {
            max_retries: 3,
            base_delay_ms: 10,
            max_delay_ms: 100,
        },
    }
}

// =============================================================================
// Error Type Tests
// =============================================================================

#[test]
fn test_error_from_http_status_429() {
    let err = FirestoreError::from_http_status(429, "rate limited");
    assert!(matches!(err, FirestoreError::RateLimited(_)));
    assert!(err.is_retryable());
}

#[test]
fn test_error_from_http_status_500() {
    let err = FirestoreError::from_http_status(500, "internal error");
    assert!(matches!(err, FirestoreError::ServerError(500, _)));
    assert!(err.is_retryable());
}

#[test]
fn test_error_from_http_status_503() {
    let err = FirestoreError::from_http_status(503, "service unavailable");
    assert!(matches!(err, FirestoreError::ServerError(503, _)));
    assert!(err.is_retryable());
}

#[test]
fn test_error_from_http_status_400() {
    let err = FirestoreError::from_http_status(400, "bad request");
    assert!(matches!(err, FirestoreError::RequestFailed(_)));
    assert!(!err.is_retryable());
}

#[test]
fn test_error_from_http_status_404() {
    let err = FirestoreError::from_http_status(404, "not found");
    assert!(matches!(err, FirestoreError::NotFound(_)));
    assert!(!err.is_retryable());
}

#[test]
fn test_error_from_http_status_409() {
    let err = FirestoreError::from_http_status(409, "conflict");
    assert!(matches!(err, FirestoreError::AlreadyExists(_)));
    assert!(!err.is_retryable());
}

#[test]
fn test_error_http_status_getter() {
    assert_eq!(FirestoreError::RateLimited(1000).http_status(), Some(429));
    assert_eq!(
        FirestoreError::ServerError(502, "bad gateway".into()).http_status(),
        Some(502)
    );
    assert_eq!(
        FirestoreError::NotFound("doc".into()).http_status(),
        Some(404)
    );
}

#[test]
fn test_error_retry_after_ms() {
    assert_eq!(FirestoreError::RateLimited(5000).retry_after_ms(), Some(5000));
    assert_eq!(
        FirestoreError::ServerError(500, "error".into()).retry_after_ms(),
        None
    );
}

// =============================================================================
// Retry Policy Tests
// =============================================================================

#[tokio::test]
async fn test_retry_logic_retries_on_server_errors() {
    let err = FirestoreError::from_http_status(500, "Internal Server Error");
    assert!(err.is_retryable(), "500 errors should be retryable");

    let err = FirestoreError::from_http_status(502, "Bad Gateway");
    assert!(err.is_retryable(), "502 errors should be retryable");

    let err = FirestoreError::from_http_status(503, "Service Unavailable");
    assert!(err.is_retryable(), "503 errors should be retryable");

    let err = FirestoreError::from_http_status(429, "Too Many Requests");
    assert!(err.is_retryable(), "429 errors should be retryable");
}

#[tokio::test]
async fn test_no_retry_on_400() {
    let err = FirestoreError::from_http_status(400, "bad request");
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn test_no_retry_on_404() {
    let err = FirestoreError::from_http_status(404, "not found");
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn test_retry_honors_rate_limit() {
    let err = FirestoreError::RateLimited(2000);
    assert!(err.is_retryable());
    assert_eq!(err.retry_after_ms(), Some(2000));
}

// =============================================================================
// Config Tests
// =============================================================================

#[test]
#[serial]
fn test_config_validates_empty_project_id() {
    std::env::set_var("GCP_PROJECT_ID", "");
    std::env::remove_var("FIREBASE_PROJECT_ID");
    let result = FirestoreConfig::from_env();
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_config_accepts_firebase_project_id() {
    std::env::remove_var("GCP_PROJECT_ID");
    std::env::set_var("FIREBASE_PROJECT_ID", "firebase-project");
    let config = FirestoreConfig::from_env().unwrap();
    assert_eq!(config.project_id, "firebase-project");
}

#[test]
#[serial]
fn test_config_prefers_gcp_project_id() {
    std::env::set_var("GCP_PROJECT_ID", "gcp-project");
    std::env::set_var("FIREBASE_PROJECT_ID", "firebase-project");
    let config = FirestoreConfig::from_env().unwrap();
    assert_eq!(config.project_id, "gcp-project");
}

#[test]
#[serial]
fn test_config_parses_timeout_env_vars() {
    std::env::set_var("GCP_PROJECT_ID", "test");
    std::env::set_var("FIRESTORE_CONNECT_TIMEOUT_SECS", "15");
    let config = FirestoreConfig::from_env().unwrap();
    assert_eq!(config.connect_timeout, Duration::from_secs(15));
}

#[test]
#[serial]
fn test_config_parses_retry_env_vars() {
    std::env::set_var("GCP_PROJECT_ID", "test");
    std::env::set_var("FIRESTORE_RETRY_BASE_MS", "50");
    std::env::set_var("FIRESTORE_RETRY_MAX_MS", "2000");
    let config = FirestoreConfig::from_env().unwrap();
    assert_eq!(config.retry.base_delay_ms, 50);
    assert_eq!(config.retry.max_delay_ms, 2000);
}

#[test]
#[serial]
fn test_config_handles_invalid_env_values() {
    std::env::set_var("GCP_PROJECT_ID", "test");
    std::env::set_var("FIRESTORE_CONNECT_TIMEOUT_SECS", "not-a-number");
    let config = FirestoreConfig::from_env().unwrap();
    assert_eq!(config.connect_timeout, Duration::from_secs(5));
}
