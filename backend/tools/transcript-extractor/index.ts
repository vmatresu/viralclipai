/**
 * Transcript Extractor Module
 *
 * Re-exports all public APIs for the transcript extraction service.
 */

// Types
export * from "./types/index.js";

// Service
export {
    TranscriptService, extractTranscript,
    getTranscriptService, type TranscriptServiceConfig
} from "./service/index.js";

// Strategies (for advanced usage)
export {
    ApifyScraperStrategy,
    TranscriptStrategy,
    WatchPageStrategy,
    YouTubeApiStrategy,
    YoutubeiStrategy,
    YtdlpStrategy
} from "./strategies/index.js";

