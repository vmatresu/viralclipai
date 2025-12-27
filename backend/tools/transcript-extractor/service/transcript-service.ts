/**
 * Multi-Strategy Transcript Service
 *
 * Orchestrates transcript extraction using multiple fallback strategies:
 * 1. Watch page (lightweight, primary) - direct HTTPS + youtubei endpoint
 * 2. youtubei.js (fast, secondary) - memory-intensive, disabled in low-memory environments
 * 3. yt-dlp (robust fallback) - uses external process, lower memory footprint
 * 4. YouTube Data API v3 (official API fallback) - requires API key
 * 5. Apify YouTube Scraper (last resort) - external API
 *
 * Architecture follows:
 * - Strategy Pattern: Interchangeable extraction methods
 * - Chain of Responsibility: Fallback through strategies
 * - Circuit Breaker: Prevent cascading failures
 * - Memory-Aware Selection: Adapts to container memory limits
 */

import { existsSync } from "node:fs";
import {
  ApifyScraperStrategy,
  TranscriptStrategy,
  WatchPageStrategy,
  YouTubeApiStrategy,
  YoutubeiStrategy,
  YtdlpStrategy,
} from "../strategies/index.js";
import {
  DEFAULT_TRANSCRIPT_OPTIONS,
  isTranscriptError,
  isTranscriptResult,
  TranscriptErrorType,
  type StrategyMemoryAssessment as MemoryAssessment,
  type TranscriptOptions,
  type TranscriptOutcome
} from "../types/index.js";
import { CircuitBreaker, CircuitState } from "../utils/circuit-breaker.js";
import { Config } from "../utils/config.js";
import { logger } from "../utils/logger.js";
import { assessStrategyAvailability } from "../utils/memory.js";
import { extractVideoIdSimple } from "../utils/youtube-url-parser.js";

/**
 * Memory requirements for youtubei.js strategy (in MB)
 *
 * Based on observed memory usage during Innertube.create() initialization.
 * The library parses YouTube's JavaScript which causes significant memory spikes.
 * In containers, the assessStrategyAvailability function applies a 1.5x safety margin.
 */
const YOUTUBEI_REQUIRED_MEMORY_MB = 1024;

/**
 * Environment variable to explicitly disable youtubei strategy
 */
const YOUTUBEI_DISABLE_ENV = "TRANSCRIPT_DISABLE_YOUTUBEI";

/**
 * Environment variable to override memory threshold (MB)
 */
const YOUTUBEI_MIN_MEMORY_ENV = "TRANSCRIPT_YOUTUBEI_MIN_MEMORY_MB";

/**
 * Evaluate whether youtubei strategy should be enabled based on memory constraints
 */
function evaluateYoutubeiAvailability(): MemoryAssessment {
  const customThreshold = process.env[YOUTUBEI_MIN_MEMORY_ENV];
  const requiredMb = customThreshold
    ? Math.max(
        Number.parseInt(customThreshold, 10) || YOUTUBEI_REQUIRED_MEMORY_MB,
        256
      )
    : YOUTUBEI_REQUIRED_MEMORY_MB;

  return assessStrategyAvailability(requiredMb, YOUTUBEI_DISABLE_ENV);
}

/**
 * Configuration for TranscriptService
 */
export interface TranscriptServiceConfig {
  /** Enable/disable caching */
  enableCache?: boolean;
  /** Custom strategies (for testing) */
  strategies?: TranscriptStrategy[];
  /** yt-dlp path (if not in PATH) */
  ytdlpPath?: string;
  /** Path to cookies file for yt-dlp */
  cookiesPath?: string;
}

/**
 * Multi-strategy transcript extraction service
 */
export class TranscriptService {
  private readonly strategies: TranscriptStrategy[];
  private readonly circuitBreaker: CircuitBreaker;

  constructor(config: TranscriptServiceConfig = {}) {
    this.circuitBreaker = new CircuitBreaker({
      name: "TranscriptService",
      failureThreshold: 10,
      timeout: 120000, // 2 minutes
    });

    // Evaluate youtubei availability at construction time
    const youtubeiAvailability = evaluateYoutubeiAvailability();
    const useDefaultStrategies = !config.strategies;

    if (useDefaultStrategies && !youtubeiAvailability.enabled) {
      logger.warn(
        {
          reason: youtubeiAvailability.reason,
          availableMb: youtubeiAvailability.availableMb,
          requiredMb: youtubeiAvailability.requiredMb,
        },
        "Youtubei strategy disabled due to memory constraints"
      );
    } else if (useDefaultStrategies) {
      logger.info(
        {
          reason: youtubeiAvailability.reason,
          availableMb: youtubeiAvailability.availableMb,
          requiredMb: youtubeiAvailability.requiredMb,
        },
        "Youtubei strategy enabled"
      );
    }

    const configuredCookiesPath =
      config.cookiesPath?.trim() || Config.ytdlpCookiesPath?.trim() || "";
    let cookiesPath: string | undefined;
    if (configuredCookiesPath && existsSync(configuredCookiesPath)) {
      cookiesPath = configuredCookiesPath;
    }

    if (configuredCookiesPath && !cookiesPath) {
      logger.warn(
        { cookiesPath: configuredCookiesPath },
        "yt-dlp cookies file not found, proceeding without cookies"
      );
    }

    // Initialize strategies in priority order
    this.strategies = config.strategies ?? [
      new WatchPageStrategy(),
      ...(youtubeiAvailability.enabled ? [new YoutubeiStrategy()] : []),
      new YtdlpStrategy({
        ytdlpPath: config.ytdlpPath,
        cookiesPath,
        usePOTokenProvider: Config.ytdlpPOTokenEnabled,
      }),
      new YouTubeApiStrategy(),
      new ApifyScraperStrategy(),
    ];

    // Sort by priority
    this.strategies.sort((a, b) => a.getPriority() - b.getPriority());
  }

  /**
   * Extract transcript from a YouTube video URL or video ID
   */
  async extractTranscript(
    videoUrlOrId: string,
    options: TranscriptOptions = {}
  ): Promise<TranscriptOutcome> {
    const mergedOptions = { ...DEFAULT_TRANSCRIPT_OPTIONS, ...options };
    const startTime = Date.now();

    // Extract video ID
    const videoId = this.extractVideoId(videoUrlOrId);
    if (!videoId) {
      return {
        success: false,
        error: "Invalid YouTube URL or video ID",
        errorType: TranscriptErrorType.UNKNOWN,
      };
    }

    logger.info(
      { videoId, strategies: this.strategies.map((s) => s.getName()) },
      "Starting transcript extraction"
    );

    // Track errors and strategy position for degradation metrics
    const errors: Array<{
      strategy: string;
      error: string;
      type: TranscriptErrorType;
      position: number;
    }> = [];

    let strategyPosition = 0;

    for (const strategy of this.strategies) {
      // Skip disabled strategies
      if (!strategy.isEnabled()) {
        logger.debug(
          { strategy: strategy.getName() },
          "Strategy disabled, skipping"
        );
        continue;
      }

      // Check availability
      const available = await strategy.isAvailable();
      if (!available) {
        logger.debug(
          { strategy: strategy.getName() },
          "Strategy not available, skipping"
        );
        continue;
      }

      try {
        strategyPosition++;

        logger.info(
          {
            videoId,
            strategy: strategy.getName(),
            position: strategyPosition,
            totalStrategies: this.strategies.length,
          },
          "Trying extraction strategy"
        );

        // Execute through circuit breaker
        const result = await this.circuitBreaker.execute(() =>
          strategy.extract(videoId, mergedOptions)
        );

        if (isTranscriptResult(result)) {
          const totalDuration = Date.now() - startTime;
          const wasDegraded = strategyPosition > 1;

          logger.info(
            {
              videoId,
              strategy: strategy.getName(),
              totalDuration,
              transcriptLength: result.transcript.length,
              strategyPosition,
              degraded: wasDegraded,
              fallbackCount: strategyPosition - 1,
            },
            wasDegraded
              ? "Transcript extraction successful (degraded)"
              : "Transcript extraction successful"
          );

          return result;
        }

        // Handle error result
        if (isTranscriptError(result)) {
          errors.push({
            strategy: strategy.getName(),
            error: result.error,
            type: result.errorType,
            position: strategyPosition,
          });

          // Check if error is permanent (no point trying other strategies)
          if (this.isPermanentError(result.errorType)) {
            logger.info(
              {
                videoId,
                strategy: strategy.getName(),
                errorType: result.errorType,
              },
              "Permanent error, stopping fallback chain"
            );
            return result;
          }

          logger.info(
            { videoId, strategy: strategy.getName(), error: result.error },
            "Strategy failed, trying next"
          );
        }
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        errors.push({
          strategy: strategy.getName(),
          error: errorMessage,
          type: TranscriptErrorType.UNKNOWN,
          position: strategyPosition,
        });

        // Check for circuit breaker open
        const isCircuitOpen = errorMessage.includes("Circuit breaker");
        logger.warn(
          {
            videoId,
            strategy: strategy.getName(),
            error: errorMessage,
            circuitOpen: isCircuitOpen,
          },
          isCircuitOpen
            ? "Circuit breaker open, skipping strategy"
            : "Strategy threw exception"
        );
      }
    }

    // All strategies failed
    const totalDuration = Date.now() - startTime;
    logger.error(
      { videoId, totalDuration, errors },
      "All transcript extraction strategies failed"
    );

    // Return the most informative error
    const bestError = this.selectBestError(errors);
    return {
      success: false,
      error: bestError.error,
      errorType: bestError.type,
    };
  }

  /**
   * Extract video ID from URL or return if already an ID
   */
  private extractVideoId(input: string): string | null {
    const trimmed = input.trim();

    // Check if it's already a valid 11-char video ID
    if (/^[A-Za-z0-9_-]{11}$/.test(trimmed)) {
      return trimmed;
    }

    // Try to extract from URL
    return extractVideoIdSimple(trimmed);
  }

  /**
   * Check if error type indicates we shouldn't try other strategies
   *
   * Note: AGE_RESTRICTED is NOT considered permanent because the YouTube API
   * strategy might be able to fetch captions for age-restricted videos where
   * yt-dlp cannot without cookies.
   *
   * Note: PARSE_ERROR is NOT permanent - it indicates youtubei.js internal
   * issues (YouTube API changes) so we should try fallback strategies.
   */
  private isPermanentError(errorType: TranscriptErrorType): boolean {
    return [
      TranscriptErrorType.VIDEO_PRIVATE,
      TranscriptErrorType.VIDEO_UNAVAILABLE,
      TranscriptErrorType.VIDEO_LIVE,
    ].includes(errorType);
  }

  /**
   * Select the most informative error from multiple failures
   */
  private selectBestError(
    errors: Array<{
      strategy: string;
      error: string;
      type: TranscriptErrorType;
    }>
  ): { error: string; type: TranscriptErrorType } {
    if (errors.length === 0) {
      return {
        error: "No transcript extraction strategies available",
        type: TranscriptErrorType.UNKNOWN,
      };
    }

    // Prefer specific errors over generic ones
    const priorityOrder = [
      TranscriptErrorType.NO_CAPTIONS,
      TranscriptErrorType.VIDEO_PRIVATE,
      TranscriptErrorType.VIDEO_UNAVAILABLE,
      TranscriptErrorType.VIDEO_LIVE,
      TranscriptErrorType.AGE_RESTRICTED,
      TranscriptErrorType.RATE_LIMITED,
      TranscriptErrorType.PO_TOKEN_ERROR,
      TranscriptErrorType.TIMEOUT,
      TranscriptErrorType.NETWORK_ERROR,
      TranscriptErrorType.PARSE_ERROR,
      TranscriptErrorType.UNKNOWN,
    ];

    const sorted = [...errors].sort((a, b) => {
      const aIndex = priorityOrder.indexOf(a.type);
      const bIndex = priorityOrder.indexOf(b.type);
      return aIndex - bIndex;
    });

    return {
      error: sorted[0].error,
      type: sorted[0].type,
    };
  }

  /**
   * Get health status of the service
   */
  getHealthStatus(): {
    healthy: boolean;
    circuitBreakerState: CircuitState;
    availableStrategies: string[];
  } {
    return {
      healthy: this.circuitBreaker.isHealthy(),
      circuitBreakerState: this.circuitBreaker.getState(),
      availableStrategies: this.strategies
        .filter((s) => s.isEnabled())
        .map((s) => s.getName()),
    };
  }

  /**
   * Reset circuit breaker (for recovery)
   */
  resetCircuitBreaker(): void {
    this.circuitBreaker.reset();
  }
}

// Default singleton instance
let defaultInstance: TranscriptService | null = null;

/**
 * Get the default TranscriptService instance
 */
export function getTranscriptService(): TranscriptService {
  if (!defaultInstance) {
    defaultInstance = new TranscriptService();
  }
  return defaultInstance;
}

/**
 * Extract transcript using default service (convenience function)
 */
export async function extractTranscript(
  videoUrlOrId: string,
  options?: TranscriptOptions
): Promise<TranscriptOutcome> {
  return getTranscriptService().extractTranscript(videoUrlOrId, options);
}
