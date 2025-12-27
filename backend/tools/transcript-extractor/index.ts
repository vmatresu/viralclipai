/**
 * Transcript Extractor Module
 *
 * Re-exports all public APIs for the transcript extraction service.
 */

// Types
export * from "./types/index.js";

// Service
export {
  extractTranscript,
  getTranscriptService,
  TranscriptService,
  type TranscriptServiceConfig,
} from "./service/index.js";

// Strategies (for advanced usage)
export {
  ApifyScraperStrategy,
  TranscriptStrategy,
  WatchPageStrategy,
  YouTubeApiStrategy,
  YoutubeiStrategy,
  YtdlpStrategy,
} from "./strategies/index.js";

// PO Token module
export * from "./po-token/index.js";

// Health check utilities
export {
  checkMemory,
  checkPOToken,
  isAlive,
  isReady,
  performHealthCheck,
  type MemoryHealthStatus,
  type POTokenHealthStatus,
  type TranscriptServiceHealth,
} from "./utils/health-check.js";

