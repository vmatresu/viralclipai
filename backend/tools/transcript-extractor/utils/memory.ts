/**
 * Memory Assessment Utility
 *
 * Assesses available memory for strategy selection in containerized environments.
 * Helps disable memory-intensive strategies (like youtubei.js) when resources are limited.
 *
 * @see https://developers.redhat.com/articles/2025/10/10/nodejs-20-memory-management-containers
 */

import { existsSync, readFileSync } from "node:fs";
import { StrategyMemoryAssessment } from "../types/index.js";

/** cgroups v2 memory limit path */
const CGROUP_V2_MEMORY_PATH = "/sys/fs/cgroup/memory.max";

/** cgroups v1 memory limit path */
const CGROUP_V1_MEMORY_PATH = "/sys/fs/cgroup/memory/memory.limit_in_bytes";

/** Docker container indicator path */
const DOCKER_ENV_PATH = "/.dockerenv";

/** Max int for detecting "unlimited" in cgroups v1 */
const MAX_SAFE_BYTES = 9007199254740991;

/**
 * Get available memory in megabytes
 *
 * In containerized environments, this returns the container's memory limit.
 * Falls back to system free memory if not in a container.
 */
function getAvailableMemoryMb(): number {
  // Try cgroups v2 first (modern Docker/Kubernetes)
  try {
    if (existsSync(CGROUP_V2_MEMORY_PATH)) {
      const content = readFileSync(CGROUP_V2_MEMORY_PATH, "utf8").trim();
      if (content !== "max") {
        const bytes = Number.parseInt(content, 10);
        if (!Number.isNaN(bytes)) {
          return Math.floor(bytes / 1024 / 1024);
        }
      }
    }
  } catch {
    // Continue to next method
  }

  // Try cgroups v1
  try {
    if (existsSync(CGROUP_V1_MEMORY_PATH)) {
      const content = readFileSync(CGROUP_V1_MEMORY_PATH, "utf8").trim();
      const bytes = Number.parseInt(content, 10);
      // Check for "unlimited" value (very large number)
      if (!Number.isNaN(bytes) && bytes < MAX_SAFE_BYTES) {
        return Math.floor(bytes / 1024 / 1024);
      }
    }
  } catch {
    // Continue to fallback
  }

  // Fall back to heap statistics (not in container or can't read cgroup info)
  const heapStats = process.memoryUsage();
  const totalHeapMb = Math.floor(heapStats.heapTotal / 1024 / 1024);
  const usedHeapMb = Math.floor(heapStats.heapUsed / 1024 / 1024);

  // Estimate available as 2x current heap (conservative)
  return Math.max(totalHeapMb - usedHeapMb, 256);
}

/**
 * Check if running in a container
 */
function isContainerized(): boolean {
  return (
    existsSync(CGROUP_V2_MEMORY_PATH) ||
    existsSync(CGROUP_V1_MEMORY_PATH) ||
    existsSync(DOCKER_ENV_PATH)
  );
}

/**
 * Assess whether a memory-intensive strategy should be enabled
 *
 * @param requiredMb - Memory required by the strategy in MB
 * @param disableEnvVar - Environment variable name to explicitly disable
 * @returns Assessment result with reason
 */
export function assessStrategyAvailability(
  requiredMb: number,
  disableEnvVar?: string
): StrategyMemoryAssessment {
  // Check explicit disable via environment variable
  if (disableEnvVar && process.env[disableEnvVar] === "true") {
    return {
      enabled: false,
      reason: `Explicitly disabled via ${disableEnvVar}`,
    };
  }

  const availableMb = getAvailableMemoryMb();
  const inContainer = isContainerized();

  // Apply safety margin for containers (1.5x requirement)
  const effectiveRequiredMb = inContainer
    ? Math.ceil(requiredMb * 1.5)
    : requiredMb;

  if (availableMb < effectiveRequiredMb) {
    return {
      enabled: false,
      reason: `Insufficient memory (${availableMb}MB available, ${effectiveRequiredMb}MB required)`,
      availableMb,
      requiredMb: effectiveRequiredMb,
    };
  }

  return {
    enabled: true,
    reason: `Sufficient memory (${availableMb}MB available, ${effectiveRequiredMb}MB required)`,
    availableMb,
    requiredMb: effectiveRequiredMb,
  };
}
