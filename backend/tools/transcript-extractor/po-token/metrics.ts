/**
 * PO Token Metrics Collection
 *
 * Collects and exposes metrics for PO token operations.
 * Designed to integrate with Prometheus or similar monitoring systems.
 */

import { POTokenErrorCode, type POTokenMetricsSnapshot } from "./types.js";

/** Maximum latency samples to keep for percentile calculations */
const MAX_LATENCY_SAMPLES = 1000;

/** Window for requests per minute calculation */
const RPM_WINDOW_MS = 60_000;

/**
 * PO Token Metrics Collector
 */
export class POTokenMetrics {
  private totalRequests = 0;
  private successfulRequests = 0;
  private failedRequests = 0;
  private cacheHits = 0;
  private cacheMisses = 0;
  private latencies: number[] = [];
  private errorsByType: Map<string, number> = new Map();
  private lastSuccessAt: number | null = null;
  private lastFailureAt: number | null = null;
  private recentRequests: number[] = [];

  /**
   * Record a successful token request
   */
  recordSuccess(latencyMs: number, cached: boolean): void {
    this.totalRequests++;
    this.successfulRequests++;
    this.lastSuccessAt = Date.now();
    this.recordLatency(latencyMs);
    this.recordRecentRequest();

    if (cached) {
      this.cacheHits++;
    } else {
      this.cacheMisses++;
    }
  }

  /**
   * Record a failed token request
   */
  recordFailure(errorCode: POTokenErrorCode | string): void {
    this.totalRequests++;
    this.failedRequests++;
    this.lastFailureAt = Date.now();
    this.recordRecentRequest();

    const key = String(errorCode);
    const current = this.errorsByType.get(key) || 0;
    this.errorsByType.set(key, current + 1);
  }

  /**
   * Record latency sample
   */
  private recordLatency(latencyMs: number): void {
    this.latencies.push(latencyMs);

    // Keep only the most recent samples
    if (this.latencies.length > MAX_LATENCY_SAMPLES) {
      this.latencies.shift();
    }
  }

  /**
   * Record request timestamp for RPM calculation
   */
  private recordRecentRequest(): void {
    const now = Date.now();
    this.recentRequests.push(now);

    // Clean old requests
    const cutoff = now - RPM_WINDOW_MS;
    this.recentRequests = this.recentRequests.filter((t) => t > cutoff);
  }

  /**
   * Calculate percentile from latency samples
   */
  private getPercentile(percentile: number): number {
    if (this.latencies.length === 0) return 0;

    const sorted = [...this.latencies].sort((a, b) => a - b);
    const index = Math.ceil((percentile / 100) * sorted.length) - 1;
    return sorted[Math.max(0, index)];
  }

  /**
   * Get metrics snapshot
   */
  getSnapshot(): POTokenMetricsSnapshot {
    const avgLatencyMs =
      this.latencies.length > 0
        ? this.latencies.reduce((a, b) => a + b, 0) / this.latencies.length
        : 0;

    const totalCacheRequests = this.cacheHits + this.cacheMisses;
    const cacheHitRatio =
      totalCacheRequests > 0 ? this.cacheHits / totalCacheRequests : 0;

    // Clean old requests for accurate RPM
    const now = Date.now();
    const cutoff = now - RPM_WINDOW_MS;
    this.recentRequests = this.recentRequests.filter((t) => t > cutoff);

    return {
      totalRequests: this.totalRequests,
      successfulRequests: this.successfulRequests,
      failedRequests: this.failedRequests,
      cacheHits: this.cacheHits,
      cacheMisses: this.cacheMisses,
      cacheHitRatio: Math.round(cacheHitRatio * 1000) / 1000,
      avgLatencyMs: Math.round(avgLatencyMs),
      p50LatencyMs: Math.round(this.getPercentile(50)),
      p95LatencyMs: Math.round(this.getPercentile(95)),
      p99LatencyMs: Math.round(this.getPercentile(99)),
      errorsByType: Object.fromEntries(this.errorsByType),
      lastSuccessAt: this.lastSuccessAt,
      lastFailureAt: this.lastFailureAt,
      requestsPerMinute: this.recentRequests.length,
    };
  }

  /**
   * Format metrics for Prometheus exposition
   */
  toPrometheusFormat(): string {
    const snapshot = this.getSnapshot();

    const errorLines = Object.entries(snapshot.errorsByType).map(
      ([errorType, count]) => `pot_errors_total{type="${errorType}"} ${count}`
    );

    const lines = [
      "# HELP pot_requests_total Total number of PO token requests",
      "# TYPE pot_requests_total counter",
      `pot_requests_total{status="success"} ${snapshot.successfulRequests}`,
      `pot_requests_total{status="failure"} ${snapshot.failedRequests}`,
      "# HELP pot_cache_hits_total Total cache hits",
      "# TYPE pot_cache_hits_total counter",
      `pot_cache_hits_total ${snapshot.cacheHits}`,
      "# HELP pot_cache_hit_ratio Cache hit ratio",
      "# TYPE pot_cache_hit_ratio gauge",
      `pot_cache_hit_ratio ${snapshot.cacheHitRatio}`,
      "# HELP pot_latency_ms Token request latency in milliseconds",
      "# TYPE pot_latency_ms summary",
      `pot_latency_ms{quantile="0.5"} ${snapshot.p50LatencyMs}`,
      `pot_latency_ms{quantile="0.95"} ${snapshot.p95LatencyMs}`,
      `pot_latency_ms{quantile="0.99"} ${snapshot.p99LatencyMs}`,
      "# HELP pot_errors_total Errors by type",
      "# TYPE pot_errors_total counter",
      ...errorLines,
      "# HELP pot_requests_per_minute Request rate",
      "# TYPE pot_requests_per_minute gauge",
      `pot_requests_per_minute ${snapshot.requestsPerMinute}`,
    ];

    return lines.join("\n");
  }

  /**
   * Reset all metrics
   */
  reset(): void {
    this.totalRequests = 0;
    this.successfulRequests = 0;
    this.failedRequests = 0;
    this.cacheHits = 0;
    this.cacheMisses = 0;
    this.latencies = [];
    this.errorsByType.clear();
    this.lastSuccessAt = null;
    this.lastFailureAt = null;
    this.recentRequests = [];
  }
}

// Global metrics singleton
let globalMetrics: POTokenMetrics | null = null;

/**
 * Get global PO token metrics instance
 */
export function getPOTokenMetrics(): POTokenMetrics {
  if (!globalMetrics) {
    globalMetrics = new POTokenMetrics();
  }
  return globalMetrics;
}

/**
 * Reset global metrics (for testing)
 */
export function resetPOTokenMetrics(): void {
  if (globalMetrics) {
    globalMetrics.reset();
  }
  globalMetrics = null;
}
