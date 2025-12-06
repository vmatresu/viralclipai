//! Metrics collection for production monitoring.
//!
//! Provides comprehensive metrics collection with support for multiple
//! backends (Prometheus, StatsD, etc.) and structured metric types.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Metric value types.
#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<f64>),
}

/// Production-ready metrics collector.
#[derive(Clone)]
pub struct ProductionMetricsCollector {
    metrics: Arc<RwLock<HashMap<String, MetricValue>>>,
    #[allow(dead_code)]
    labels: HashMap<String, Vec<(String, String)>>,
}

impl ProductionMetricsCollector {
    /// Create a new production metrics collector.
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            labels: HashMap::new(),
        }
    }

    /// Increment a counter metric.
    pub fn increment_counter(&self, name: &str, labels: &[(&str, &str)]) {
        let mut metrics = self.metrics.write().unwrap();
        let key = self.make_key(name, labels);

        let counter = metrics.entry(key).or_insert(MetricValue::Counter(0));
        if let MetricValue::Counter(ref mut value) = counter {
            *value += 1;
        }
    }

    /// Record a gauge metric.
    pub fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        let mut metrics = self.metrics.write().unwrap();
        let key = self.make_key(name, labels);
        metrics.insert(key, MetricValue::Gauge(value));
    }

    /// Record a histogram value.
    pub fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        let mut metrics = self.metrics.write().unwrap();
        let key = self.make_key(name, labels);

        let histogram = metrics.entry(key).or_insert(MetricValue::Histogram(Vec::new()));
        if let MetricValue::Histogram(ref mut values) = histogram {
            values.push(value);
        }
    }

    /// Get snapshot of all metrics.
    pub fn snapshot(&self) -> HashMap<String, MetricValue> {
        self.metrics.read().unwrap().clone()
    }

    /// Export metrics in Prometheus format.
    pub fn prometheus_export(&self) -> String {
        let mut output = String::new();
        let metrics = self.snapshot();

        for (key, value) in metrics {
            match value {
                MetricValue::Counter(count) => {
                    output.push_str(&format!("# HELP {} Counter metric\n", key));
                    output.push_str(&format!("# TYPE {} counter\n", key));
                    output.push_str(&format!("{} {}\n", key, count));
                }
                MetricValue::Gauge(value) => {
                    output.push_str(&format!("# HELP {} Gauge metric\n", key));
                    output.push_str(&format!("# TYPE {} gauge\n", key));
                    output.push_str(&format!("{} {}\n", key, value));
                }
                MetricValue::Histogram(values) => {
                    if !values.is_empty() {
                        let sum: f64 = values.iter().sum();
                        let count = values.len();
                        let avg = sum / count as f64;

                        output.push_str(&format!("# HELP {} Histogram metric\n", key));
                        output.push_str(&format!("# TYPE {} histogram\n", key));
                        output.push_str(&format!("{}_count {} {}\n", key, count, self.format_labels(&key)));
                        output.push_str(&format!("{}_sum {} {}\n", key, sum, self.format_labels(&key)));
                        output.push_str(&format!("{}_avg {} {}\n", key, avg, self.format_labels(&key)));
                    }
                }
            }
        }

        output
    }

    fn make_key(&self, name: &str, labels: &[(&str, &str)]) -> String {
        if labels.is_empty() {
            name.to_string()
        } else {
            let label_str = labels.iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}{{{}}}", name, label_str)
        }
    }

    fn format_labels(&self, key: &str) -> String {
        // Extract labels from key if present
        if let Some(start) = key.find('{') {
            if let Some(end) = key.find('}') {
                return key[start..=end].to_string();
            }
        }
        String::new()
    }
}

impl Default for ProductionMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Health check metrics.
pub struct HealthMetrics {
    collector: ProductionMetricsCollector,
}

impl HealthMetrics {
    pub fn new(collector: ProductionMetricsCollector) -> Self {
        Self { collector }
    }

    pub fn record_health_check(&self, service: &str, healthy: bool) {
        let status = if healthy { "healthy" } else { "unhealthy" };
        self.collector.increment_counter(
            "health_check_total",
            &[("service", service), ("status", status)]
        );
    }

    pub fn record_response_time(&self, service: &str, duration_ms: f64) {
        self.collector.record_histogram(
            "service_response_time_ms",
            duration_ms,
            &[("service", service)]
        );
    }
}
