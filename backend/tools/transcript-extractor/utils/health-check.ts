/**
 * Health Check Utilities
 *
 * Provides health status checking for the transcript extraction service.
 */

import { getPOTokenService } from "../po-token/index.js";

/**
 * PO Token health status
 */
export interface POTokenHealthStatus {
  status: "up" | "down" | "disabled";
  providerUrl?: string;
  circuitState?: string;
  error?: string;
  metrics?: {
    totalRequests: number;
    successfulRequests: number;
    failedRequests: number;
    cacheHitRatio: number;
    avgLatencyMs: number;
  };
}

/**
 * Memory health status
 */
export interface MemoryHealthStatus {
  status: "ok" | "warning" | "critical";
  heapUsedMb: number;
  heapTotalMb: number;
  usagePercent: number;
}

/**
 * Overall service health status
 */
export interface TranscriptServiceHealth {
  status: "healthy" | "degraded" | "unhealthy";
  timestamp: string;
  uptime: number;
  checks: {
    memory: MemoryHealthStatus;
    poToken: POTokenHealthStatus;
  };
}

/**
 * Check memory usage
 */
export function checkMemory(): MemoryHealthStatus {
  const usage = process.memoryUsage();
  const heapUsedMb = Math.round(usage.heapUsed / 1024 / 1024);
  const heapTotalMb = Math.round(usage.heapTotal / 1024 / 1024);
  const usagePercent = heapTotalMb > 0 ? (heapUsedMb / heapTotalMb) * 100 : 0;

  let status: MemoryHealthStatus["status"] = "ok";

  // Thresholds based on typical container limits
  const warningUsageAbsolute = heapUsedMb > 256;
  const criticalUsageAbsolute = heapUsedMb > 512;

  if (criticalUsageAbsolute && usagePercent > 90) {
    status = "critical";
  } else if (warningUsageAbsolute && usagePercent > 80) {
    status = "warning";
  }

  return {
    status,
    heapUsedMb,
    heapTotalMb,
    usagePercent: Math.round(usagePercent),
  };
}

/**
 * Check PO Token provider status
 */
export function checkPOToken(): POTokenHealthStatus {
  try {
    const service = getPOTokenService();
    const status = service.getStatus();

    if (!status.enabled) {
      return { status: "disabled" };
    }

    if (!status.providerHealthy) {
      return {
        status: "down",
        providerUrl: status.providerUrl,
        circuitState: status.circuitState,
        error: "Provider health check failed",
      };
    }

    return {
      status: "up",
      providerUrl: status.providerUrl,
      circuitState: status.circuitState,
      metrics: {
        totalRequests: status.metrics.totalRequests,
        successfulRequests: status.metrics.successfulRequests,
        failedRequests: status.metrics.failedRequests,
        cacheHitRatio: status.metrics.cacheHitRatio,
        avgLatencyMs: status.metrics.avgLatencyMs,
      },
    };
  } catch (error) {
    return {
      status: "down",
      error: error instanceof Error ? error.message : "Service unavailable",
    };
  }
}

/**
 * Perform comprehensive health check
 */
export function performHealthCheck(): TranscriptServiceHealth {
  const memoryCheck = checkMemory();
  const poTokenCheck = checkPOToken();

  // Determine overall status
  let status: TranscriptServiceHealth["status"] = "healthy";

  if (memoryCheck.status === "critical") {
    status = "unhealthy";
  } else if (memoryCheck.status === "warning" || poTokenCheck.status === "down") {
    status = "degraded";
  }

  return {
    status,
    timestamp: new Date().toISOString(),
    uptime: Math.floor(process.uptime()),
    checks: {
      memory: memoryCheck,
      poToken: poTokenCheck,
    },
  };
}

/**
 * Simple readiness check (for load balancer)
 */
export function isReady(): boolean {
  const memoryCheck = checkMemory();
  return memoryCheck.status !== "critical";
}

/**
 * Simple liveness check (for orchestrator)
 */
export function isAlive(): boolean {
  return true; // If this code runs, the process is alive
}
