/**
 * PO Token Module
 *
 * Provides PO (Proof of Origin) token generation and management
 * for YouTube video server authentication.
 *
 * @example
 * ```typescript
 * import { getPOTokenService } from './po-token';
 *
 * const service = getPOTokenService();
 * const result = await service.getToken({ videoId: 'abc123' });
 * if (result.success) {
 *   console.log('Token:', result.token);
 *   console.log('yt-dlp args:', result.extractorArgs);
 * }
 * ```
 */

// Types
export {
  POTokenError,
  POTokenErrorCode,
  type POTokenMetricsSnapshot,
  type POTokenRequest,
  type POTokenResponse,
  type POTokenServiceStatus,
  type TokenContext,
  type TokenResult,
  type YouTubeClient,
} from "./types.js";

// Configuration
export {
  getPOTokenConfig,
  loadPOTokenConfig,
  resetPOTokenConfig,
  validatePOTokenConfig,
  type POTokenConfig,
} from "./config.js";

// Metrics
export {
  getPOTokenMetrics,
  POTokenMetrics,
  resetPOTokenMetrics,
} from "./metrics.js";

// Client
export { POTokenHttpClient } from "./client.js";

// Service
export {
  getPOTokenService,
  initializePOTokenService,
  POTokenService,
  resetPOTokenService,
  shutdownPOTokenService,
} from "./service.js";
