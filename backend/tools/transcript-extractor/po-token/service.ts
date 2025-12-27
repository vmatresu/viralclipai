/**
 * PO Token Service
 *
 * High-level service for PO token management.
 * Provides a simplified interface for workers to obtain tokens.
 *
 * Features:
 * - Singleton pattern for shared state
 * - Automatic provider health monitoring
 * - Fallback mode when provider unavailable
 * - Integration with yt-dlp configuration
 */

import { logger } from "../utils/logger.js";
import { POTokenHttpClient } from "./client.js";
import {
  getPOTokenConfig,
  validatePOTokenConfig,
  type POTokenConfig,
} from "./config.js";
import { getPOTokenMetrics, POTokenMetrics } from "./metrics.js";
import {
  POTokenError,
  POTokenErrorCode,
  type POTokenResponse,
  type POTokenServiceStatus,
  type TokenContext,
  type TokenResult,
  type YouTubeClient,
} from "./types.js";

/** Health check interval in ms */
const HEALTH_CHECK_INTERVAL = 30_000;

/**
 * PO Token Service
 */
export class POTokenService {
  private readonly config: POTokenConfig;
  private readonly client: POTokenHttpClient;
  private readonly metrics: POTokenMetrics;
  private providerHealthy = false;
  private lastHealthCheck: number | null = null;
  private healthCheckInterval: ReturnType<typeof setInterval> | null = null;

  constructor(config?: POTokenConfig) {
    this.config = config ?? getPOTokenConfig();
    this.metrics = getPOTokenMetrics();
    this.client = new POTokenHttpClient(this.config, this.metrics);

    // Validate configuration
    const errors = validatePOTokenConfig(this.config);
    if (errors.length > 0) {
      logger.error({ errors }, "Invalid PO Token configuration");
      throw new Error(`PO Token configuration errors: ${errors.join(", ")}`);
    }

    logger.info(
      {
        enabled: this.config.enabled,
        providerUrl: this.config.providerUrl,
        defaultClient: this.config.defaultClient,
      },
      "PO Token service initialized"
    );
  }

  /**
   * Start background health monitoring
   */
  startHealthMonitoring(): void {
    if (!this.config.enabled) {
      logger.info("PO Token service disabled, skipping health monitoring");
      return;
    }

    // Initial health check
    this.checkProviderHealth().catch((error) => {
      logger.warn(
        { error: error instanceof Error ? error.message : String(error) },
        "Initial PO token provider health check failed"
      );
    });

    // Periodic health checks
    this.healthCheckInterval = setInterval(
      () => this.checkProviderHealth(),
      HEALTH_CHECK_INTERVAL
    );

    logger.info(
      { intervalMs: HEALTH_CHECK_INTERVAL },
      "PO Token health monitoring started"
    );
  }

  /**
   * Stop health monitoring
   */
  stopHealthMonitoring(): void {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
      this.healthCheckInterval = null;
      logger.info("PO Token health monitoring stopped");
    }
  }

  /**
   * Check provider health
   */
  private async checkProviderHealth(): Promise<void> {
    try {
      const result = await this.client.healthCheck();
      this.providerHealthy = result.healthy;
      this.lastHealthCheck = Date.now();

      if (!result.healthy) {
        logger.warn(
          { error: result.error, latencyMs: result.latencyMs },
          "PO token provider unhealthy"
        );
      } else if (this.config.debug) {
        logger.debug(
          { latencyMs: result.latencyMs },
          "PO token provider healthy"
        );
      }
    } catch (error) {
      this.providerHealthy = false;
      this.lastHealthCheck = Date.now();
      logger.error(
        { error: error instanceof Error ? error.message : String(error) },
        "PO token provider health check failed"
      );
    }
  }

  /**
   * Get a PO token for yt-dlp
   *
   * Returns formatted result ready for use with yt-dlp extractor-args.
   */
  async getToken(options?: {
    videoId?: string;
    client?: YouTubeClient;
    context?: TokenContext;
    correlationId?: string;
  }): Promise<TokenResult> {
    if (!this.config.enabled) {
      return {
        success: false,
        error: "PO Token service is disabled",
        degraded: true,
      };
    }

    const client = options?.client ?? this.config.defaultClient;
    const context = options?.context ?? this.config.defaultContext;

    try {
      const response = await this.client.getToken({
        videoId: options?.videoId,
        client,
        context,
        correlationId: options?.correlationId,
      });

      return {
        success: true,
        token: response.token,
        extractorArgs: this.formatExtractorArgs(response),
        degraded: false,
        metadata: {
          client: response.client,
          context: response.context,
          expiresAt: response.expiresAt,
          cached: response.cached,
        },
      };
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      const errorCode =
        error instanceof POTokenError ? error.code : POTokenErrorCode.GENERATION_FAILED;

      logger.warn(
        { error: errorMessage, errorCode, videoId: options?.videoId },
        "Failed to obtain PO token"
      );

      return {
        success: false,
        error: errorMessage,
        degraded: true,
      };
    }
  }

  /**
   * Format extractor-args for yt-dlp
   *
   * Format: youtube:player_client=CLIENT;po_token=CLIENT.CONTEXT+TOKEN
   */
  private formatExtractorArgs(response: POTokenResponse): string {
    const { token, client, context } = response;
    return `youtube:player_client=${client};po_token=${client}.${context}+${token}`;
  }

  /**
   * Get yt-dlp arguments for PO token usage
   *
   * Returns array of command-line arguments for yt-dlp.
   */
  async getYtdlpArgs(options?: {
    videoId?: string;
    client?: YouTubeClient;
    context?: TokenContext;
    correlationId?: string;
  }): Promise<string[]> {
    const result = await this.getToken(options);

    if (!result.success || !result.extractorArgs) {
      // Return fallback args without PO token
      // Uses web_creator which works for subtitles without tokens
      return ["--extractor-args", "youtube:player_client=web_creator"];
    }

    return ["--extractor-args", result.extractorArgs];
  }

  /**
   * Invalidate provider cache
   */
  async invalidateCache(): Promise<void> {
    if (!this.config.enabled) {
      throw new Error("PO Token service is disabled");
    }
    await this.client.invalidateCache();
  }

  /**
   * Get service status
   */
  getStatus(): POTokenServiceStatus {
    return {
      enabled: this.config.enabled,
      providerHealthy: this.providerHealthy,
      lastHealthCheck: this.lastHealthCheck,
      circuitState: this.client.getCircuitState(),
      providerUrl: this.config.providerUrl,
      metrics: this.metrics.getSnapshot(),
    };
  }

  /**
   * Check if service is ready to serve requests
   */
  isReady(): boolean {
    if (!this.config.enabled) {
      return true; // Disabled is a valid ready state
    }
    return this.providerHealthy;
  }

  /**
   * Cleanup resources
   */
  destroy(): void {
    this.stopHealthMonitoring();
    this.client.destroy();
    logger.info("PO Token service destroyed");
  }
}

// Singleton instance
let serviceInstance: POTokenService | null = null;

/**
 * Get the PO Token service instance
 */
export function getPOTokenService(): POTokenService {
  if (!serviceInstance) {
    serviceInstance = new POTokenService();
  }
  return serviceInstance;
}

/**
 * Initialize the PO Token service
 * Call this during application startup.
 */
export function initializePOTokenService(): POTokenService {
  const service = getPOTokenService();
  service.startHealthMonitoring();
  return service;
}

/**
 * Shutdown the PO Token service
 * Call this during graceful shutdown.
 */
export function shutdownPOTokenService(): void {
  if (serviceInstance) {
    serviceInstance.destroy();
    serviceInstance = null;
  }
}

/**
 * Reset the PO Token service (for testing)
 */
export function resetPOTokenService(): void {
  if (serviceInstance) {
    serviceInstance.destroy();
    serviceInstance = null;
  }
}
