//! Retry policy with exponential backoff and jitter.
//!
//! Implements production-grade retry behavior:
//! - Exponential backoff with full jitter
//! - Respects Retry-After header on 429
//! - Configurable base and max delays

use std::time::Duration;

use tracing::{info_span, warn, Instrument};

use crate::error::{FirestoreError, FirestoreResult};
use crate::metrics::record_retry;

// =============================================================================
// Configuration
// =============================================================================

/// Retry policy configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Base delay for exponential backoff (in milliseconds).
    pub base_delay_ms: u64,
    /// Maximum delay cap (in milliseconds).
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
        }
    }
}

impl RetryConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        let base_delay_ms: u64 = std::env::var("FIRESTORE_RETRY_BASE_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);

        let max_delay_ms: u64 = std::env::var("FIRESTORE_RETRY_MAX_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5000);

        Self {
            max_retries: 3,
            base_delay_ms,
            max_delay_ms,
        }
    }
}

// =============================================================================
// Retry Policy
// =============================================================================

/// Execute an async operation with retry.
///
/// Retries on:
/// - Network errors
/// - HTTP 429 (Too Many Requests) - honors Retry-After header
/// - HTTP 5xx (Server errors)
///
/// Does NOT retry:
/// - HTTP 4xx (except 429)
/// - Auth errors
/// - Already exists / not found
pub async fn with_retry<T, F, Fut>(
    config: &RetryConfig,
    operation: &str,
    op: F,
) -> FirestoreResult<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = FirestoreResult<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        let span = info_span!("firestore_retry", operation = %operation, attempt = attempt + 1);

        let result = op().instrument(span).await;

        match result {
            Ok(value) => return Ok(value),
            Err(e) if e.is_retryable() && attempt < config.max_retries => {
                // Calculate delay with full jitter
                let delay = calculate_delay(config, attempt, e.retry_after_ms());

                warn!(
                    operation = %operation,
                    attempt = attempt + 1,
                    delay_ms = delay.as_millis() as u64,
                    "Firestore operation failed, retrying: {}",
                    e
                );

                record_retry(operation);

                tokio::time::sleep(delay).await;
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_error.unwrap_or_else(|| FirestoreError::request_failed("Unknown error")))
}

/// Calculate retry delay with exponential backoff and full jitter.
fn calculate_delay(config: &RetryConfig, attempt: u32, retry_after_ms: Option<u64>) -> Duration {
    // Honor Retry-After header if present
    if let Some(after) = retry_after_ms {
        return Duration::from_millis(after);
    }

    // Calculate exponential backoff: base * 2^attempt
    let exp_delay = config.base_delay_ms.saturating_mul(2u64.pow(attempt));
    let capped_delay = exp_delay.min(config.max_delay_ms);

    // Apply full jitter: random value between 0 and capped_delay
    // Using simple time-based pseudo-randomization to avoid adding rand crate
    let jittered = if capped_delay > 0 {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let random_factor = (nanos % 1000) as f64 / 1000.0;
        ((capped_delay as f64) * random_factor) as u64
    } else {
        0
    };

    // Ensure minimum delay of base_delay_ms
    Duration::from_millis(jittered.max(config.base_delay_ms))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
    }

    #[test]
    fn test_calculate_delay_with_retry_after() {
        let config = RetryConfig::default();
        let delay = calculate_delay(&config, 0, Some(2000));
        assert_eq!(delay, Duration::from_millis(2000));
    }

    #[test]
    fn test_calculate_delay_respects_max() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 2000,
        };
        // Even with high attempt count, delay should be capped
        let delay = calculate_delay(&config, 10, None);
        assert!(delay.as_millis() <= 2000);
    }

    #[test]
    fn test_calculate_delay_minimum() {
        let config = RetryConfig::default();
        let delay = calculate_delay(&config, 0, None);
        assert!(delay.as_millis() >= config.base_delay_ms as u128);
    }
}
