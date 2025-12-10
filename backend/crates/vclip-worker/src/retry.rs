//! Retry utilities with exponential backoff.
//!
//! Provides reusable retry patterns for resilient operations against
//! potentially flaky external services (Redis, S3, etc.).

use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial attempt).
    pub max_retries: u32,
    /// Base delay for exponential backoff (doubles each attempt).
    pub base_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Operation name for logging.
    pub operation_name: String,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            operation_name: "operation".to_string(),
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with the given operation name.
    pub fn new(operation_name: impl Into<String>) -> Self {
        Self {
            operation_name: operation_name.into(),
            ..Default::default()
        }
    }

    /// Set the maximum number of retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the base delay for exponential backoff.
    pub fn with_base_delay(mut self, base_delay: Duration) -> Self {
        self.base_delay = base_delay;
        self
    }

    /// Calculate delay for a given attempt number.
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.base_delay.saturating_mul(2u32.pow(attempt));
        delay.min(self.max_delay)
    }
}

/// Result of a retry operation.
#[derive(Debug)]
pub enum RetryResult<T, E> {
    /// Operation succeeded.
    Success(T),
    /// Operation failed after all retries exhausted.
    Failed { error: E, attempts: u32 },
}

impl<T, E> RetryResult<T, E> {
    /// Returns true if the operation succeeded.
    pub fn is_success(&self) -> bool {
        matches!(self, RetryResult::Success(_))
    }

    /// Unwrap the success value or panic.
    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            RetryResult::Success(v) => v,
            RetryResult::Failed { error, attempts } => {
                panic!("Operation failed after {} attempts: {:?}", attempts, error)
            }
        }
    }
}

/// Execute an async operation with retry logic.
///
/// # Type Parameters
/// - `F`: Factory function that returns a future
/// - `Fut`: The future type
/// - `T`: Success type
/// - `E`: Error type (must implement Display)
///
/// # Example
/// ```ignore
/// let config = RetryConfig::new("redis_heartbeat").with_max_retries(3);
/// let result = retry_async(&config, || async {
///     redis_client.ping().await
/// }).await;
/// ```
pub async fn retry_async<F, Fut, T, E>(config: &RetryConfig, operation: F) -> RetryResult<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0u32;

    loop {
        match operation().await {
            Ok(value) => return RetryResult::Success(value),
            Err(e) if attempt < config.max_retries => {
                attempt += 1;
                let delay = config.delay_for_attempt(attempt);
                debug!(
                    "{} attempt {} failed, retrying in {:?}: {}",
                    config.operation_name, attempt, delay, e
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                return RetryResult::Failed {
                    error: e,
                    attempts: attempt + 1,
                }
            }
        }
    }
}

/// State tracker for repeated operations that may fail intermittently.
///
/// Useful for background tasks (like heartbeats) that run continuously
/// and should suppress log spam after repeated failures.
#[derive(Debug, Default)]
pub struct FailureTracker {
    consecutive_failures: u32,
    max_logged_failures: u32,
    suppressed: bool,
}

impl FailureTracker {
    /// Create a new failure tracker.
    pub fn new(max_logged_failures: u32) -> Self {
        Self {
            consecutive_failures: 0,
            max_logged_failures,
            suppressed: false,
        }
    }

    /// Record a successful operation (resets failure count).
    pub fn record_success(&mut self) {
        if self.consecutive_failures > 0 && self.suppressed {
            // Log recovery after suppression
            debug!(
                "Operation recovered after {} consecutive failures",
                self.consecutive_failures
            );
        }
        self.consecutive_failures = 0;
        self.suppressed = false;
    }

    /// Record a failed operation.
    ///
    /// Returns `true` if this failure should be logged (not suppressed).
    pub fn record_failure(&mut self) -> bool {
        self.consecutive_failures += 1;

        if self.consecutive_failures <= self.max_logged_failures {
            true
        } else if self.consecutive_failures == self.max_logged_failures + 1 {
            // Log the suppression message once
            self.suppressed = true;
            warn!(
                "Suppressing further failure logs after {} consecutive failures",
                self.max_logged_failures
            );
            false
        } else {
            false
        }
    }

    /// Get the current consecutive failure count.
    pub fn failure_count(&self) -> u32 {
        self.consecutive_failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig::new("test").with_base_delay(Duration::from_millis(100));

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn test_retry_config_max_delay() {
        let config = RetryConfig::new("test")
            .with_base_delay(Duration::from_secs(1))
            .with_max_retries(10);

        // Should cap at max_delay (5s by default)
        let delay = config.delay_for_attempt(10);
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn test_failure_tracker_suppression() {
        let mut tracker = FailureTracker::new(3);

        // First 3 failures should be logged
        assert!(tracker.record_failure());
        assert!(tracker.record_failure());
        assert!(tracker.record_failure());

        // 4th failure triggers suppression message (returns false)
        assert!(!tracker.record_failure());

        // Subsequent failures are suppressed
        assert!(!tracker.record_failure());
        assert!(!tracker.record_failure());

        // Success resets
        tracker.record_success();
        assert_eq!(tracker.failure_count(), 0);

        // New failures are logged again
        assert!(tracker.record_failure());
    }

    #[tokio::test]
    async fn test_retry_async_immediate_success() {
        let config = RetryConfig::new("test");
        let call_count = std::sync::atomic::AtomicU32::new(0);

        let result = retry_async(&config, || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Ok::<_, String>(42) }
        })
        .await;

        assert!(result.is_success());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_async_eventual_success() {
        let config = RetryConfig::new("test").with_base_delay(Duration::from_millis(1));
        let call_count = std::sync::atomic::AtomicU32::new(0);

        let result = retry_async(&config, || {
            let count = call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                if count < 2 {
                    Err("transient error")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_success());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
