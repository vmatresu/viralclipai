/**
 * Base Transcript Extraction Strategy
 *
 * Defines the interface that all transcript extraction strategies must implement.
 * Follows the Strategy Pattern for interchangeable extraction methods.
 */

import {
    TranscriptErrorType,
    type StrategyConfig,
    type TranscriptOptions,
    type TranscriptOutcome,
} from "../types/index.js";

/**
 * Classify an error message into a TranscriptErrorType
 * Shared utility used by all transcript strategies
 */
export function classifyTranscriptError(message: string): TranscriptErrorType {
  const lower = message.toLowerCase();

  // youtubei parser/runtime class generation failures
  // These are internal library errors that indicate YouTube API changes
  // Short-circuit to fallbacks immediately when detected
  if (
    lower.includes("type mismatch") ||
    lower.includes("singlecolumnwatchnextresults") ||
    lower.includes("largevideocontrols") ||
    lower.includes("slimvideometadatasection") ||
    lower.includes("cannot read properties of null") ||
    lower.includes("cannot read properties of undefined") ||
    lower.includes("module not found") ||
    lower.includes("innertubeerror")
  ) {
    return TranscriptErrorType.PARSE_ERROR;
  }

  if (lower.includes("live") || lower.includes("upcoming")) {
    return TranscriptErrorType.VIDEO_LIVE;
  }
  if (lower.includes("private")) {
    return TranscriptErrorType.VIDEO_PRIVATE;
  }
  if (lower.includes("unavailable") || lower.includes("deleted")) {
    return TranscriptErrorType.VIDEO_UNAVAILABLE;
  }
  if (lower.includes("age") || lower.includes("sign in")) {
    return TranscriptErrorType.AGE_RESTRICTED;
  }
  if (lower.includes("no transcript") || lower.includes("no captions")) {
    return TranscriptErrorType.NO_CAPTIONS;
  }
  if (lower.includes("rate limit") || lower.includes("quota")) {
    return TranscriptErrorType.RATE_LIMITED;
  }
  if (lower.includes("timeout")) {
    return TranscriptErrorType.TIMEOUT;
  }
  if (lower.includes("network") || lower.includes("fetch")) {
    return TranscriptErrorType.NETWORK_ERROR;
  }
  if (lower.includes("parse") || lower.includes("parser")) {
    return TranscriptErrorType.PARSE_ERROR;
  }

  return TranscriptErrorType.UNKNOWN;
}

/**
 * Abstract base class for transcript extraction strategies
 */
export abstract class TranscriptStrategy {
  protected readonly config: StrategyConfig;

  constructor(config: Partial<StrategyConfig>) {
    this.config = {
      name: config.name ?? "unnamed",
      timeoutMs: config.timeoutMs ?? 30000,
      enabled: config.enabled ?? true,
      priority: config.priority ?? 100,
    };
  }

  /**
   * Extract transcript from a video
   * @param videoId - YouTube video ID (11 characters)
   * @param options - Extraction options
   */
  abstract extract(
    videoId: string,
    options: TranscriptOptions
  ): Promise<TranscriptOutcome>;

  /**
   * Check if this strategy is available (e.g., dependencies installed)
   */
  abstract isAvailable(): Promise<boolean>;

  /**
   * Get strategy name
   */
  getName(): string {
    return this.config.name;
  }

  /**
   * Get strategy priority (lower = higher priority)
   */
  getPriority(): number {
    return this.config.priority;
  }

  /**
   * Check if strategy is enabled
   */
  isEnabled(): boolean {
    return this.config.enabled;
  }

  /**
   * Get timeout for this strategy
   */
  getTimeout(): number {
    return this.config.timeoutMs;
  }
}

/**
 * Strategy execution result with metadata
 */
export interface StrategyExecutionResult {
  outcome: TranscriptOutcome;
  strategyName: string;
  durationMs: number;
  error?: Error;
}
