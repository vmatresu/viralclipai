/**
 * PO Token Provider Configuration
 *
 * Centralized configuration for PO token provider integration.
 * All values can be overridden via environment variables.
 */

import type { TokenContext, YouTubeClient } from "./types.js";

/**
 * PO Token configuration interface
 */
export interface POTokenConfig {
  /** Whether PO token feature is enabled */
  enabled: boolean;

  /** Base URL of the PO token provider HTTP server */
  providerUrl: string;

  /** Token TTL in seconds (for client-side caching validation) */
  tokenTtlSeconds: number;

  /** Request timeout in milliseconds */
  requestTimeoutMs: number;

  /** Number of retry attempts on failure */
  retryAttempts: number;

  /** Base delay for exponential backoff (ms) */
  retryBaseDelayMs: number;

  /** Number of failures before circuit breaker opens */
  circuitBreakerFailureThreshold: number;

  /** Number of successes to close circuit breaker */
  circuitBreakerSuccessThreshold: number;

  /** Time in ms before circuit breaker attempts reset */
  circuitBreakerResetTimeout: number;

  /** Default YouTube client for token generation */
  defaultClient: YouTubeClient;

  /** Default token context */
  defaultContext: TokenContext;

  /** Enable verbose logging for debugging */
  debug: boolean;
}

/**
 * Parse boolean from environment variable
 */
const parseBoolean = (value: string | undefined, defaultValue: boolean): boolean => {
  if (value === undefined) return defaultValue;
  return value.toLowerCase() === "true";
};

/**
 * Parse integer from environment variable
 */
const parseNumber = (value: string | undefined, defaultValue: number): number => {
  if (value === undefined) return defaultValue;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : defaultValue;
};

/**
 * Validate YouTube client type
 */
const isValidClient = (value: string): value is YouTubeClient => {
  return ["web", "mweb", "web_creator", "tv", "tv_embedded"].includes(value);
};

/**
 * Validate token context
 */
const isValidContext = (value: string): value is TokenContext => {
  return ["gvs", "player", "subs"].includes(value);
};

/**
 * Load PO Token configuration from environment
 */
export function loadPOTokenConfig(): POTokenConfig {
  const nodeEnv = process.env.NODE_ENV || "development";
  const defaultClient = process.env.POT_DEFAULT_CLIENT || "mweb";
  const defaultContext = process.env.POT_DEFAULT_CONTEXT || "gvs";

  return {
    enabled: parseBoolean(process.env.POT_ENABLED, true),
    // Default to Docker service name; for local dev, set POT_PROVIDER_URL=http://localhost:8080
    providerUrl: process.env.POT_PROVIDER_URL || "http://pot-provider:8080",
    tokenTtlSeconds: parseNumber(process.env.POT_TOKEN_TTL_SECONDS, 21600), // 6 hours
    requestTimeoutMs: parseNumber(process.env.POT_REQUEST_TIMEOUT_MS, 30000),
    retryAttempts: parseNumber(process.env.POT_RETRY_ATTEMPTS, 3),
    retryBaseDelayMs: parseNumber(process.env.POT_RETRY_BASE_DELAY_MS, 1000),
    circuitBreakerFailureThreshold: parseNumber(
      process.env.POT_CIRCUIT_FAILURE_THRESHOLD,
      5
    ),
    circuitBreakerSuccessThreshold: parseNumber(
      process.env.POT_CIRCUIT_SUCCESS_THRESHOLD,
      2
    ),
    circuitBreakerResetTimeout: parseNumber(
      process.env.POT_CIRCUIT_RESET_TIMEOUT_MS,
      60000
    ),
    defaultClient: isValidClient(defaultClient) ? defaultClient : "mweb",
    defaultContext: isValidContext(defaultContext) ? defaultContext : "gvs",
    debug: parseBoolean(process.env.POT_DEBUG, nodeEnv !== "production"),
  };
}

/**
 * Validate PO Token configuration
 */
export function validatePOTokenConfig(config: POTokenConfig): string[] {
  const errors: string[] = [];

  if (config.enabled) {
    try {
      new URL(config.providerUrl);
    } catch {
      errors.push(`Invalid POT_PROVIDER_URL: ${config.providerUrl}`);
    }

    if (config.tokenTtlSeconds < 60) {
      errors.push("POT_TOKEN_TTL_SECONDS should be at least 60 seconds");
    }

    if (config.requestTimeoutMs < 1000) {
      errors.push("POT_REQUEST_TIMEOUT_MS should be at least 1000ms");
    }

    if (config.retryAttempts < 0 || config.retryAttempts > 10) {
      errors.push("POT_RETRY_ATTEMPTS should be between 0 and 10");
    }

    if (!isValidClient(config.defaultClient)) {
      errors.push(`Invalid POT_DEFAULT_CLIENT: ${config.defaultClient}`);
    }

    if (!isValidContext(config.defaultContext)) {
      errors.push(`Invalid POT_DEFAULT_CONTEXT: ${config.defaultContext}`);
    }
  }

  return errors;
}

// Configuration singleton
let configInstance: POTokenConfig | null = null;

/**
 * Get the PO Token configuration (singleton)
 */
export function getPOTokenConfig(): POTokenConfig {
  if (!configInstance) {
    configInstance = loadPOTokenConfig();
  }
  return configInstance;
}

/**
 * Reset configuration (for testing)
 */
export function resetPOTokenConfig(): void {
  configInstance = null;
}
