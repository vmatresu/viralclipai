/**
 * PO Token HTTP Client
 *
 * Production-grade client for requesting PO (Proof of Origin) tokens
 * from the bgutil-ytdlp-pot-provider HTTP server.
 *
 * Features:
 * - Circuit breaker for fault tolerance
 * - Exponential backoff with jitter
 * - Request timeout handling
 * - Structured logging with correlation IDs
 * - Metrics collection
 *
 * @see https://github.com/Brainicism/bgutil-ytdlp-pot-provider
 */

import http from "node:http";
import { URL } from "node:url";
import { randomUUID } from "node:crypto";

import { CircuitBreaker, withRetry, withTimeout } from "../utils/circuit-breaker.js";
import { logger } from "../utils/logger.js";
import type { POTokenConfig } from "./config.js";
import { POTokenMetrics } from "./metrics.js";
import {
  POTokenError,
  POTokenErrorCode,
  type POTokenRequest,
  type POTokenResponse,
  type TokenContext,
  type YouTubeClient,
} from "./types.js";

/**
 * HTTP Agent configuration for connection pooling
 */
const createHttpAgent = (): http.Agent => {
  return new http.Agent({
    keepAlive: true,
    keepAliveMsecs: 30_000,
    maxSockets: 10,
    maxFreeSockets: 5,
    timeout: 60_000,
  });
};

/**
 * PO Token HTTP Client
 *
 * Resilient client for the bgutil PO token provider service.
 */
export class POTokenHttpClient {
  private readonly config: POTokenConfig;
  private readonly circuitBreaker: CircuitBreaker;
  private readonly httpAgent: http.Agent;
  private readonly metrics: POTokenMetrics;
  private readonly providerUrl: URL;

  constructor(config: POTokenConfig, metrics?: POTokenMetrics) {
    this.config = config;
    this.metrics = metrics ?? new POTokenMetrics();
    this.httpAgent = createHttpAgent();
    this.providerUrl = new URL(config.providerUrl);

    this.circuitBreaker = new CircuitBreaker({
      name: "po-token-provider",
      failureThreshold: config.circuitBreakerFailureThreshold,
      successThreshold: config.circuitBreakerSuccessThreshold,
      timeout: config.circuitBreakerResetTimeout,
    });

    logger.info(
      {
        providerUrl: config.providerUrl,
        timeoutMs: config.requestTimeoutMs,
        retryAttempts: config.retryAttempts,
      },
      "PO Token HTTP client initialized"
    );
  }

  /**
   * Request a PO token from the provider
   */
  async getToken(request: POTokenRequest = {}): Promise<POTokenResponse> {
    const correlationId = request.correlationId ?? randomUUID();
    const startTime = Date.now();

    const context = request.context ?? "gvs";
    const client = request.client ?? "mweb";

    logger.debug(
      { correlationId, videoId: request.videoId, context, client },
      "Requesting PO token"
    );

    try {
      const response = await this.circuitBreaker.execute(
        async () => this.executeTokenRequest(request, correlationId),
        "getToken"
      );

      const latencyMs = Date.now() - startTime;
      this.metrics.recordSuccess(latencyMs, response.cached);

      logger.info(
        { correlationId, latencyMs, cached: response.cached, client: response.client },
        "PO token obtained"
      );

      return response;
    } catch (error) {
      const latencyMs = Date.now() - startTime;
      this.metrics.recordFailure(this.classifyError(error));

      logger.error(
        {
          correlationId,
          latencyMs,
          error: error instanceof Error ? error.message : String(error),
          errorCode: error instanceof POTokenError ? error.code : "UNKNOWN",
        },
        "Failed to obtain PO token"
      );

      throw error;
    }
  }

  /**
   * Execute the token request with retries
   */
  private async executeTokenRequest(
    request: POTokenRequest,
    correlationId: string
  ): Promise<POTokenResponse> {
    const operation = async (): Promise<POTokenResponse> => {
      return withTimeout(
        this.makeHttpRequest(request, correlationId),
        this.config.requestTimeoutMs,
        `PO token request timed out after ${this.config.requestTimeoutMs}ms`
      );
    };

    if (this.config.retryAttempts > 0) {
      return withRetry(operation, {
        maxRetries: this.config.retryAttempts,
        baseDelayMs: this.config.retryBaseDelayMs,
        operationName: "PO token request",
      });
    }

    return operation();
  }

  /**
   * Make the actual HTTP request to the provider
   */
  private makeHttpRequest(
    request: POTokenRequest,
    correlationId: string
  ): Promise<POTokenResponse> {
    return new Promise((resolve, reject) => {
      const context = request.context ?? "gvs";
      const client = request.client ?? "mweb";

      // Build request URL with query parameters
      const url = new URL("/get_pot", this.providerUrl);
      if (request.videoId) {
        url.searchParams.set("video_id", request.videoId);
      }
      url.searchParams.set("context", context);
      url.searchParams.set("client", client);

      const options: http.RequestOptions = {
        hostname: this.providerUrl.hostname,
        port: this.providerUrl.port || 8080,
        path: url.pathname + url.search,
        method: "GET",
        agent: this.httpAgent,
        timeout: this.config.requestTimeoutMs,
        headers: {
          Accept: "application/json",
          "X-Correlation-ID": correlationId,
          "User-Agent": "transcript-extractor-po-token-client/1.0",
        },
      };

      const req = http.request(options, (res) => {
        let data = "";

        res.on("data", (chunk) => {
          data += chunk;
        });

        res.on("end", () => {
          try {
            if (res.statusCode === 429) {
              reject(
                new POTokenError(
                  "Rate limited by PO token provider",
                  POTokenErrorCode.RATE_LIMITED,
                  429,
                  true,
                  correlationId
                )
              );
              return;
            }

            const statusCode = res.statusCode ?? 0;
            if (statusCode !== 200) {
              reject(
                new POTokenError(
                  `Provider returned status ${statusCode}: ${data}`,
                  POTokenErrorCode.GENERATION_FAILED,
                  statusCode,
                  statusCode >= 500,
                  correlationId
                )
              );
              return;
            }

            const response = this.parseResponse(data, client, context, correlationId);
            resolve(response);
          } catch (error) {
            reject(error);
          }
        });
      });

      req.on("error", (error) => {
        const isConnectError =
          error.message.includes("ECONNREFUSED") ||
          error.message.includes("ENOTFOUND") ||
          error.message.includes("ETIMEDOUT");

        reject(
          new POTokenError(
            `Provider connection failed: ${error.message}`,
            POTokenErrorCode.PROVIDER_UNAVAILABLE,
            undefined,
            isConnectError,
            correlationId
          )
        );
      });

      req.on("timeout", () => {
        req.destroy();
        reject(
          new POTokenError(
            `Request timed out after ${this.config.requestTimeoutMs}ms`,
            POTokenErrorCode.TIMEOUT,
            undefined,
            true,
            correlationId
          )
        );
      });

      req.end();
    });
  }

  /**
   * Parse the provider response
   */
  private parseResponse(
    data: string,
    client: YouTubeClient,
    context: TokenContext,
    correlationId: string
  ): POTokenResponse {
    try {
      // bgutil provider returns plain token text or JSON
      let token: string;
      let cached = false;
      let expiresAt = Date.now() + this.config.tokenTtlSeconds * 1000;

      if (data.startsWith("{")) {
        const json = JSON.parse(data);
        token = json.token || json.po_token || json.potoken;
        cached = json.cached ?? false;
        if (json.expires_at) {
          expiresAt = json.expires_at;
        } else if (json.ttl) {
          expiresAt = Date.now() + json.ttl * 1000;
        }
      } else {
        // Plain text token response
        token = data.trim();
      }

      if (!token || token.length < 10) {
        throw new POTokenError(
          "Invalid token received from provider",
          POTokenErrorCode.INVALID_RESPONSE,
          undefined,
          false,
          correlationId
        );
      }

      return { token, expiresAt, cached, client, context };
    } catch (error) {
      if (error instanceof POTokenError) throw error;
      throw new POTokenError(
        `Failed to parse provider response: ${error instanceof Error ? error.message : String(error)}`,
        POTokenErrorCode.INVALID_RESPONSE,
        undefined,
        false,
        correlationId
      );
    }
  }

  /**
   * Classify error type for metrics
   */
  private classifyError(error: unknown): POTokenErrorCode {
    if (error instanceof POTokenError) return error.code;
    if (error instanceof Error) {
      if (error.message.includes("Circuit breaker is")) {
        return POTokenErrorCode.CIRCUIT_OPEN;
      }
      if (error.message.includes("timed out")) {
        return POTokenErrorCode.TIMEOUT;
      }
    }
    return POTokenErrorCode.GENERATION_FAILED;
  }

  /**
   * Check if the provider is healthy
   */
  async healthCheck(): Promise<{ healthy: boolean; latencyMs: number; error?: string }> {
    const startTime = Date.now();

    try {
      await this.ping();
      return { healthy: true, latencyMs: Date.now() - startTime };
    } catch (error) {
      return {
        healthy: false,
        latencyMs: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Ping the provider health endpoint
   */
  private ping(): Promise<void> {
    return new Promise((resolve, reject) => {
      const options: http.RequestOptions = {
        hostname: this.providerUrl.hostname,
        port: this.providerUrl.port || 8080,
        path: "/ping",
        method: "GET",
        agent: this.httpAgent,
        timeout: 5000,
      };

      const req = http.request(options, (res) => {
        if (res.statusCode === 200) {
          resolve();
        } else {
          reject(new Error(`Health check failed: status ${res.statusCode}`));
        }
        res.resume(); // Drain the response
      });

      req.on("error", reject);
      req.on("timeout", () => {
        req.destroy();
        reject(new Error("Health check timed out"));
      });

      req.end();
    });
  }

  /**
   * Invalidate the provider's token cache
   */
  async invalidateCache(): Promise<void> {
    return new Promise((resolve, reject) => {
      const options: http.RequestOptions = {
        hostname: this.providerUrl.hostname,
        port: this.providerUrl.port || 8080,
        path: "/invalidate",
        method: "POST",
        agent: this.httpAgent,
        timeout: 5000,
      };

      const req = http.request(options, (res) => {
        if (res.statusCode === 200 || res.statusCode === 204) {
          logger.info("PO token cache invalidated");
          resolve();
        } else {
          reject(new Error(`Cache invalidation failed: status ${res.statusCode}`));
        }
        res.resume();
      });

      req.on("error", reject);
      req.end();
    });
  }

  /**
   * Get circuit breaker state for monitoring
   */
  getCircuitState(): string {
    return this.circuitBreaker.getState();
  }

  /**
   * Get current metrics
   */
  getMetrics(): ReturnType<POTokenMetrics["getSnapshot"]> {
    return this.metrics.getSnapshot();
  }

  /**
   * Cleanup resources
   */
  destroy(): void {
    this.httpAgent.destroy();
    logger.info("PO Token HTTP client destroyed");
  }
}
