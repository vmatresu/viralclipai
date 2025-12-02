/**
 * Security constants
 * Centralized security-related constants
 */

/**
 * Maximum URL length to prevent DoS attacks
 */
export const MAX_URL_LENGTH = 2048;

/**
 * Maximum custom prompt length
 */
export const MAX_PROMPT_LENGTH = 5000;

/**
 * Maximum WebSocket message size (in bytes)
 */
export const MAX_WEBSOCKET_MESSAGE_SIZE = 1024 * 1024; // 1MB

/**
 * Allowed video URL patterns
 */
export const ALLOWED_VIDEO_DOMAINS = [
  "youtube.com",
  "www.youtube.com",
  "youtu.be",
  "tiktok.com",
  "www.tiktok.com",
  "vm.tiktok.com",
] as const;

/**
 * Rate limiting constants
 */
export const RATE_LIMIT = {
  REQUESTS_PER_MINUTE: 10,
  REQUESTS_PER_HOUR: 100,
} as const;

/**
 * Content Security Policy directives
 */
export const CSP_DIRECTIVES = {
  defaultSrc: ["'self'"],
  scriptSrc: [
    "'self'",
    "'unsafe-inline'", // Required for Next.js
    "'unsafe-eval'", // Required for Next.js in development
    "https://www.googletagmanager.com",
    "https://www.google-analytics.com",
  ],
  styleSrc: ["'self'", "'unsafe-inline'"],
  imgSrc: ["'self'", "data:", "https:", "blob:"],
  connectSrc: ["'self'", "https://*.googleapis.com", "wss://*", "ws://*"],
  fontSrc: ["'self'", "data:"],
  objectSrc: ["'none'"],
  mediaSrc: ["'self'", "blob:", "https:"],
  frameSrc: ["'none'"],
  baseUri: ["'self'"],
  formAction: ["'self'"],
  frameAncestors: ["'none'"],
  upgradeInsecureRequests: true,
} as const;
