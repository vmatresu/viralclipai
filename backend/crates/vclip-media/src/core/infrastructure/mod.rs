//! Infrastructure utilities for production monitoring.
//!
//! This module provides:
//! - Circuit breaker for external services
//! - Health monitoring and alerts
//! - Service discovery and registration
//! - Infrastructure metrics collection

pub mod circuit_breaker;
pub mod metrics;

pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use metrics::ProductionMetricsCollector;
