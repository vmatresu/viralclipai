/**
 * Circuit Breaker Pattern Implementation
 *
 * Provides resilience for external service calls by:
 * - Tracking failures and preventing cascading failures
 * - Automatically recovering after a timeout period
 * - Providing health status information
 * - Retry with exponential backoff
 * - Error classification for intelligent retry decisions
 *
 * States:
 * - CLOSED: Normal operation, requests pass through
 * - OPEN: Service failing, requests rejected immediately
 * - HALF_OPEN: Testing if service recovered
 */

import { logger } from "./logger.js";

// ============================================================================
// Types & Enums
// ============================================================================

export enum CircuitState {
  CLOSED = "closed", // Normal operation
  OPEN = "open", // Failures exceeded threshold, rejecting calls
  HALF_OPEN = "half_open", // Testing if service recovered
}

export interface CircuitBreakerConfig {
  name: string;
  /** Number of failures before opening circuit */
  failureThreshold: number;
  /** Time in ms before attempting recovery */
  timeout: number;
  /** Number of successes in half-open before closing */
  successThreshold?: number;
}

// ============================================================================
// Error Classification
// ============================================================================

export enum ErrorType {
  RETRYABLE = "retryable",
  PERMANENT = "permanent",
  RATE_LIMIT = "rate_limit",
  TIMEOUT = "timeout",
  LIVE_VIDEO = "live_video",
}

export interface ClassifiedError {
  type: ErrorType;
  message: string;
  retryable: boolean;
}

/**
 * Classify error to determine if it should be retried
 */
export function classifyError(error: unknown): ClassifiedError {
  const errorMessage = error instanceof Error ? error.message : String(error);
  const lowerMessage = errorMessage.toLowerCase();

  // Live/upcoming videos
  if (
    lowerMessage.includes("live") ||
    lowerMessage.includes("upcoming") ||
    lowerMessage.includes("premiere")
  ) {
    return {
      type: ErrorType.LIVE_VIDEO,
      message: "This is a live or upcoming video. Transcripts may not be available yet.",
      retryable: false,
    };
  }

  // Rate limiting
  if (
    lowerMessage.includes("rate limit") ||
    lowerMessage.includes("quota") ||
    lowerMessage.includes("429")
  ) {
    return {
      type: ErrorType.RATE_LIMIT,
      message: "YouTube API rate limit reached. Please try again later.",
      retryable: true,
    };
  }

  // Timeout
  if (lowerMessage.includes("timeout") || lowerMessage.includes("timed out")) {
    return {
      type: ErrorType.TIMEOUT,
      message: "Operation timed out. Please try again.",
      retryable: true,
    };
  }

  // Permanent errors (no captions, unavailable, private, etc.)
  if (
    lowerMessage.includes("no transcript") ||
    lowerMessage.includes("no captions") ||
    lowerMessage.includes("unavailable") ||
    lowerMessage.includes("private") ||
    lowerMessage.includes("deleted") ||
    lowerMessage.includes("removed") ||
    lowerMessage.includes("age") ||
    lowerMessage.includes("sign in")
  ) {
    return {
      type: ErrorType.PERMANENT,
      message: errorMessage,
      retryable: false,
    };
  }

  // Network/temporary errors (default to retryable)
  if (
    lowerMessage.includes("network") ||
    lowerMessage.includes("econnrefused") ||
    lowerMessage.includes("enotfound") ||
    lowerMessage.includes("etimedout") ||
    lowerMessage.includes("socket") ||
    lowerMessage.includes("fetch failed")
  ) {
    return {
      type: ErrorType.RETRYABLE,
      message: "Network error occurred. Retrying...",
      retryable: true,
    };
  }

  // Server errors (5xx) are retryable
  if (
    lowerMessage.includes("status 500") ||
    lowerMessage.includes("status 502") ||
    lowerMessage.includes("status 503") ||
    lowerMessage.includes("status 504") ||
    lowerMessage.includes("internal server error") ||
    lowerMessage.includes("bad gateway") ||
    lowerMessage.includes("service unavailable")
  ) {
    return {
      type: ErrorType.RETRYABLE,
      message: "Server error occurred. Retrying...",
      retryable: true,
    };
  }

  // Check for explicit retryable property on error objects
  if (
    error instanceof Error &&
    "retryable" in error &&
    (error as { retryable: boolean }).retryable === true
  ) {
    return {
      type: ErrorType.RETRYABLE,
      message: errorMessage,
      retryable: true,
    };
  }

  // Unknown errors - be cautious and don't retry
  return {
    type: ErrorType.PERMANENT,
    message: errorMessage,
    retryable: false,
  };
}

// ============================================================================
// Circuit Breaker Implementation
// ============================================================================

/**
 * Circuit Breaker implementation
 */
export class CircuitBreaker {
  private state: CircuitState = CircuitState.CLOSED;
  private failureCount = 0;
  private successCount = 0;
  private lastFailureTime = 0;
  private readonly config: Required<CircuitBreakerConfig>;

  constructor(config: CircuitBreakerConfig) {
    this.config = {
      successThreshold: 2,
      ...config,
    };
  }

  /**
   * Execute an operation through the circuit breaker
   */
  async execute<T>(
    operation: () => Promise<T>,
    operationName?: string
  ): Promise<T> {
    const opName = operationName ?? "operation";

    if (this.state === CircuitState.OPEN) {
      // Check if we should transition to half-open
      if (Date.now() - this.lastFailureTime >= this.config.timeout) {
        logger.info(
          { circuitBreaker: this.config.name, operationName: opName },
          "Circuit breaker entering half-open state"
        );
        this.state = CircuitState.HALF_OPEN;
        this.successCount = 0;
      } else {
        logger.warn(
          { circuitBreaker: this.config.name, operationName: opName, failureCount: this.failureCount },
          "Circuit breaker rejecting request"
        );
        throw new Error(
          `Circuit breaker ${this.config.name} is open, rejecting call`
        );
      }
    }

    try {
      const result = await operation();
      this.onSuccess(opName);
      return result;
    } catch (error) {
      this.onFailure(opName);
      throw error;
    }
  }

  private onSuccess(operationName: string): void {
    this.failureCount = 0;

    if (this.state === CircuitState.HALF_OPEN) {
      this.successCount++;
      if (this.successCount >= this.config.successThreshold) {
        logger.info(
          { circuitBreaker: this.config.name, operationName },
          "Circuit breaker closing after successful recovery"
        );
        this.state = CircuitState.CLOSED;
        this.successCount = 0;
      }
    }
  }

  private onFailure(operationName: string): void {
    this.failureCount++;
    this.lastFailureTime = Date.now();

    if (this.failureCount >= this.config.failureThreshold) {
      logger.error(
        { circuitBreaker: this.config.name, operationName, failureCount: this.failureCount },
        "Circuit breaker opening due to repeated failures"
      );
      this.state = CircuitState.OPEN;
    }
  }

  /**
   * Check if circuit is healthy (closed or half-open)
   */
  isHealthy(): boolean {
    return this.state !== CircuitState.OPEN;
  }

  /**
   * Get current state
   */
  getState(): CircuitState {
    return this.state;
  }

  /**
   * Reset circuit breaker to closed state
   */
  reset(): void {
    this.state = CircuitState.CLOSED;
    this.failureCount = 0;
    this.successCount = 0;
    this.lastFailureTime = 0;
  }
}

// ============================================================================
// Retry Logic
// ============================================================================

export interface RetryConfig {
  maxRetries?: number;
  baseDelayMs?: number;
  operationName?: string;
  /** Maximum delay cap in ms (default: 30000) */
  maxDelayMs?: number;
  /** Whether to add jitter to delays (default: true) */
  jitter?: boolean;
}

const DEFAULT_RETRY_CONFIG: Required<RetryConfig> = {
  maxRetries: 3,
  baseDelayMs: 1000,
  operationName: "operation",
  maxDelayMs: 30000,
  jitter: true,
};

/**
 * Add jitter to a delay value (up to 25% variance)
 */
function addJitter(delayMs: number): number {
  const jitterFactor = 0.25;
  const jitter = delayMs * jitterFactor * (Math.random() * 2 - 1);
  return Math.max(0, delayMs + jitter);
}

/**
 * Execute operation with exponential backoff retry
 */
export async function withRetry<T>(
  operation: () => Promise<T>,
  config: RetryConfig = {}
): Promise<T> {
  const merged = { ...DEFAULT_RETRY_CONFIG, ...config };
  let lastError: Error | unknown;
  let lastClassifiedError: ClassifiedError | null = null;

  for (let attempt = 1; attempt <= merged.maxRetries + 1; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      const classified = classifyError(error);
      lastClassifiedError = classified;

      const errorMessage = error instanceof Error ? error.message : String(error);

      // Don't retry permanent errors
      if (!classified.retryable) {
        logger.warn(
          { attempt, errorType: classified.type, error: errorMessage },
          `${merged.operationName} failed with non-retryable error`
        );
        throw error;
      }

      // Retry only for retryable errors
      if (attempt <= merged.maxRetries) {
        let delay = merged.baseDelayMs * Math.pow(2, attempt - 1);
        delay = Math.min(delay, merged.maxDelayMs);
        if (merged.jitter) {
          delay = addJitter(delay);
        }

        logger.warn(
          {
            attempt,
            maxRetries: merged.maxRetries,
            errorType: classified.type,
            error: errorMessage,
            delay: Math.round(delay),
          },
          `${merged.operationName} failed with retryable error, retrying...`
        );
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }
  }

  // All retries exhausted
  logger.error(
    { errorType: lastClassifiedError?.type, retries: merged.maxRetries },
    `${merged.operationName} failed after ${merged.maxRetries} retries`
  );
  throw lastError;
}

// ============================================================================
// Timeout Helper
// ============================================================================

/**
 * Execute a promise with a timeout
 *
 * @param promise - Promise to execute
 * @param timeoutMs - Timeout in milliseconds
 * @param operationName - Name for error messages
 */
export async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  operationName = "Operation"
): Promise<T> {
  let timeoutId: NodeJS.Timeout | undefined;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => {
      reject(new Error(`${operationName} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });

  try {
    const result = await Promise.race([promise, timeoutPromise]);
    if (timeoutId) clearTimeout(timeoutId);
    return result;
  } catch (error) {
    if (timeoutId) clearTimeout(timeoutId);
    throw error;
  }
}
