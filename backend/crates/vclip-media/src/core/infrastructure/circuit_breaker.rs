//! Circuit breaker for external service calls.
//!
//! Provides fault tolerance and graceful degradation for unreliable services.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Circuit breaker states.
#[derive(Clone, Debug, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (failing fast)
    Open { opened_at: Instant },
    /// Circuit is half-open (testing recovery)
    HalfOpen { success_count: u32 },
}

/// Circuit breaker implementation for external services.
#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    #[allow(dead_code)]
    failure_threshold: u32,
    recovery_timeout: Duration,
    success_threshold: u32,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(failure_threshold: u32, recovery_timeout: Duration, success_threshold: u32) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_threshold,
            recovery_timeout,
            success_threshold,
        }
    }

    /// Check if operation is allowed.
    pub fn allow(&self) -> bool {
        let mut state = self.state.write().unwrap();
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open { opened_at } => {
                if Instant::now().duration_since(opened_at) > self.recovery_timeout {
                    *state = CircuitState::HalfOpen { success_count: 0 };
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen { .. } => true,
        }
    }

    /// Record a successful operation.
    pub fn success(&self) {
        let mut state = self.state.write().unwrap();
        match *state {
            CircuitState::HalfOpen { success_count } => {
                let new_count = success_count + 1;
                if new_count >= self.success_threshold {
                    *state = CircuitState::Closed;
                } else {
                    *state = CircuitState::HalfOpen { success_count: new_count };
                }
            }
            _ => {} // No change for other states
        }
    }

    /// Record a failed operation.
    pub fn failure(&self) {
        let mut state = self.state.write().unwrap();
        match *state {
            CircuitState::Closed | CircuitState::HalfOpen { .. } => {
                *state = CircuitState::Open { opened_at: Instant::now() };
            }
            CircuitState::Open { .. } => {} // Already open
        }
    }

    /// Get current state for monitoring.
    pub fn state(&self) -> CircuitState {
        self.state.read().unwrap().clone()
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(60), 3)
    }
}
