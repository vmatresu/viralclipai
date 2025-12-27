/**
 * PO Token Types
 *
 * Type definitions for PO (Proof of Origin) token integration.
 * Used for authenticating requests to YouTube's video servers.
 */

/**
 * Token context for different YouTube request types
 */
export type TokenContext = "gvs" | "player" | "subs";

/**
 * YouTube client types supported for PO tokens
 */
export type YouTubeClient = "web" | "mweb" | "web_creator" | "tv" | "tv_embedded";

/**
 * Request parameters for token generation
 */
export interface POTokenRequest {
  /** Video ID (optional, some tokens are video-agnostic) */
  videoId?: string;
  /** Token context (gvs for Google Video Server) */
  context?: TokenContext;
  /** YouTube client type */
  client?: YouTubeClient;
  /** Request correlation ID for tracing */
  correlationId?: string;
}

/**
 * Response from the PO token provider
 */
export interface POTokenResponse {
  /** The generated PO token */
  token: string;
  /** Token expiration timestamp (Unix ms) */
  expiresAt: number;
  /** Whether this was a cache hit */
  cached: boolean;
  /** Client type used for generation */
  client: YouTubeClient;
  /** Token context */
  context: TokenContext;
}

/**
 * Token result with formatted yt-dlp arguments
 */
export interface TokenResult {
  /** Whether token was successfully obtained */
  success: boolean;
  /** The PO token (if successful) */
  token?: string;
  /** Formatted extractor-args for yt-dlp */
  extractorArgs?: string;
  /** Error message (if failed) */
  error?: string;
  /** Whether this is a fallback/degraded mode */
  degraded: boolean;
  /** Token metadata */
  metadata?: {
    client: YouTubeClient;
    context: TokenContext;
    expiresAt: number;
    cached: boolean;
  };
}

/**
 * PO Token error codes
 */
export enum POTokenErrorCode {
  PROVIDER_UNAVAILABLE = "PROVIDER_UNAVAILABLE",
  GENERATION_FAILED = "GENERATION_FAILED",
  TIMEOUT = "TIMEOUT",
  CIRCUIT_OPEN = "CIRCUIT_OPEN",
  INVALID_RESPONSE = "INVALID_RESPONSE",
  RATE_LIMITED = "RATE_LIMITED",
}

/**
 * Error thrown when PO token generation fails
 */
export class POTokenError extends Error {
  constructor(
    message: string,
    public readonly code: POTokenErrorCode,
    public readonly statusCode?: number,
    public readonly retryable: boolean = false,
    public readonly correlationId?: string
  ) {
    super(message);
    this.name = "POTokenError";
    Object.setPrototypeOf(this, POTokenError.prototype);
  }
}

/**
 * Service status
 */
export interface POTokenServiceStatus {
  /** Whether the service is enabled */
  enabled: boolean;
  /** Whether the provider is healthy */
  providerHealthy: boolean;
  /** Last health check timestamp */
  lastHealthCheck: number | null;
  /** Circuit breaker state */
  circuitState: string;
  /** Provider URL */
  providerUrl: string;
  /** Current metrics */
  metrics: POTokenMetricsSnapshot;
}

/**
 * Metrics snapshot interface
 */
export interface POTokenMetricsSnapshot {
  /** Total number of token requests */
  totalRequests: number;
  /** Number of successful requests */
  successfulRequests: number;
  /** Number of failed requests */
  failedRequests: number;
  /** Cache hit count */
  cacheHits: number;
  /** Cache miss count */
  cacheMisses: number;
  /** Cache hit ratio (0-1) */
  cacheHitRatio: number;
  /** Average latency in ms */
  avgLatencyMs: number;
  /** P50 latency in ms */
  p50LatencyMs: number;
  /** P95 latency in ms */
  p95LatencyMs: number;
  /** P99 latency in ms */
  p99LatencyMs: number;
  /** Error counts by type */
  errorsByType: Record<string, number>;
  /** Last successful request timestamp */
  lastSuccessAt: number | null;
  /** Last failed request timestamp */
  lastFailureAt: number | null;
  /** Requests in the last minute */
  requestsPerMinute: number;
}
