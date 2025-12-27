/**
 * Circuit Breaker Pattern Implementation
 *
 * Prevents cascading failures by tracking failure rates and
 * temporarily disabling failing operations.
 */

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
  async execute<T>(operation: () => Promise<T>): Promise<T> {
    if (this.state === CircuitState.OPEN) {
      // Check if we should transition to half-open
      if (Date.now() - this.lastFailureTime >= this.config.timeout) {
        this.state = CircuitState.HALF_OPEN;
        this.successCount = 0;
      } else {
        throw new Error(
          `Circuit breaker ${this.config.name} is open, rejecting call`
        );
      }
    }

    try {
      const result = await operation();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      throw error;
    }
  }

  private onSuccess(): void {
    this.failureCount = 0;

    if (this.state === CircuitState.HALF_OPEN) {
      this.successCount++;
      if (this.successCount >= this.config.successThreshold) {
        this.state = CircuitState.CLOSED;
      }
    }
  }

  private onFailure(): void {
    this.failureCount++;
    this.lastFailureTime = Date.now();

    if (this.failureCount >= this.config.failureThreshold) {
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
