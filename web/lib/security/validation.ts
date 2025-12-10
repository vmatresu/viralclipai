/**
 * Security validation utilities
 * Provides input validation and sanitization functions
 */

import { MAX_PROMPT_LENGTH, MAX_URL_LENGTH } from "./constants";

/**
 * Validates if a string is a valid URL
 * @param url - The URL string to validate
 * @returns true if valid URL, false otherwise
 */
export function isValidUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    // Only allow http and https protocols
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Validates and sanitizes a URL
 * @param url - The URL string to validate and sanitize
 * @returns The sanitized URL or null if invalid
 */
export function sanitizeUrl(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) {
    return null;
  }

  if (!isValidUrl(trimmed)) {
    return null;
  }

  try {
    const parsed = new URL(trimmed);
    // Remove dangerous protocols and ensure only http/https
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null;
    }
    // Remove javascript:, data:, vbscript: etc. from href attributes
    if (
      parsed.href.toLowerCase().startsWith("javascript:") ||
      parsed.href.toLowerCase().startsWith("data:") ||
      parsed.href.toLowerCase().startsWith("vbscript:")
    ) {
      return null;
    }
    return parsed.href;
  } catch {
    return null;
  }
}

/**
 * Validates WebSocket URL
 * @param url - The WebSocket URL to validate
 * @returns true if valid WebSocket URL, false otherwise
 */
export function isValidWebSocketUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "ws:" || parsed.protocol === "wss:";
  } catch {
    return false;
  }
}

/**
 * Sanitizes a string to prevent XSS attacks
 * @param input - The string to sanitize
 * @returns Sanitized string
 */
export function sanitizeString(input: string): string {
  return input
    .replace(/[<>]/g, "") // Remove < and >
    .replace(/javascript:/gi, "") // Remove javascript: protocol
    .replace(/on\w+=/gi, "") // Remove event handlers like onclick=
    .trim();
}

/**
 * Validates environment variable is present and non-empty
 * @param value - The environment variable value
 * @param name - The name of the environment variable (for error messages)
 * @returns The value if valid, throws error otherwise
 */
export function requireEnv(value: string | undefined, name: string): string {
  if (!value || value.trim() === "") {
    throw new Error(`Required environment variable ${name} is missing or empty`);
  }
  return value;
}

/**
 * Validates Firebase configuration object
 * @param config - Firebase config object
 * @returns true if valid, false otherwise
 */
export function isValidFirebaseConfig(config: {
  apiKey?: string;
  authDomain?: string;
  projectId?: string;
}): boolean {
  return Boolean(
    config.apiKey &&
    config.apiKey.trim() !== "" &&
    config.authDomain &&
    config.authDomain.trim() !== "" &&
    config.projectId &&
    config.projectId.trim() !== ""
  );
}

/**
 * Validates that a string matches a specific pattern
 * @param input - The string to validate
 * @param pattern - The regex pattern to match
 * @returns true if matches, false otherwise
 */
export function matchesPattern(input: string, pattern: RegExp): boolean {
  return pattern.test(input);
}

/**
 * Limits string length to prevent DoS attacks
 * @param input - The string to limit
 * @param maxLength - Maximum allowed length
 * @returns Truncated string if exceeds maxLength
 */
export function limitLength(input: string, maxLength: number): string {
  if (input.length > maxLength) {
    return input.substring(0, maxLength);
  }
  return input;
}

// ============================================================================
// Video URL Validation (must match backend whitelist)
// ============================================================================

/**
 * Allowed video URL domains (whitelist for security)
 * This list must match the backend security.rs ALLOWED_DOMAINS
 */
const ALLOWED_VIDEO_DOMAINS = new Set([
  // YouTube
  "youtube.com",
  "www.youtube.com",
  "youtu.be",
  "m.youtube.com",
  // Vimeo
  "vimeo.com",
  "www.vimeo.com",
  "player.vimeo.com",
  // Loom
  "loom.com",
  "www.loom.com",
  // Wistia
  "wistia.com",
  "www.wistia.com",
  "fast.wistia.com",
  // Dailymotion
  "dailymotion.com",
  "www.dailymotion.com",
  // TikTok
  "tiktok.com",
  "www.tiktok.com",
  "vm.tiktok.com",
  // Twitter/X
  "twitter.com",
  "www.twitter.com",
  "x.com",
  "www.x.com",
  // Instagram
  "instagram.com",
  "www.instagram.com",
  // Facebook
  "facebook.com",
  "www.facebook.com",
  "fb.watch",
  // Twitch
  "twitch.tv",
  "www.twitch.tv",
  "clips.twitch.tv",
  // Streamable
  "streamable.com",
  "www.streamable.com",
]);

/**
 * Maximum URL length for video URLs (alias for consistency)
 */
const MAX_VIDEO_URL_LENGTH = MAX_URL_LENGTH;

/**
 * Validates a video URL for security
 * @param url - The video URL to validate
 * @returns Object with isValid flag and error message if invalid
 */
export function validateVideoUrl(url: string): {
  isValid: boolean;
  error?: string;
  sanitizedUrl?: string;
} {
  // Check length
  if (url.length > MAX_VIDEO_URL_LENGTH) {
    return { isValid: false, error: "URL is too long" };
  }

  const trimmed = url.trim();
  if (!trimmed) {
    return { isValid: false, error: "URL cannot be empty" };
  }

  // Parse URL
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return { isValid: false, error: "Invalid URL format" };
  }

  // Check protocol
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return {
      isValid: false,
      error: "Only HTTP and HTTPS URLs are allowed",
    };
  }

  // Extract domain
  const domain = parsed.hostname.toLowerCase();

  // Check domain whitelist
  if (!isDomainAllowed(domain)) {
    return {
      isValid: false,
      error: `Domain "${domain}" is not supported. Please use a supported video platform (YouTube, Vimeo, TikTok, etc.)`,
    };
  }

  return { isValid: true, sanitizedUrl: parsed.href };
}

/**
 * Check if a domain is in the allowed list
 */
function isDomainAllowed(domain: string): boolean {
  // Direct match
  if (ALLOWED_VIDEO_DOMAINS.has(domain)) {
    return true;
  }

  // Check parent domains (e.g., "video.youtube.com" is allowed because "youtube.com" is)
  const parts = domain.split(".");
  if (parts.length >= 2) {
    const parent = `${parts[parts.length - 2]}.${parts[parts.length - 1]}`;
    if (ALLOWED_VIDEO_DOMAINS.has(parent)) {
      return true;
    }
  }

  return false;
}

/**
 * Sanitizes a prompt string for safe submission
 * @param prompt - The prompt to sanitize
 * @returns Sanitized prompt
 */
export function sanitizePrompt(prompt: string): string {
  // Remove control characters except newline and tab
  const cleaned = prompt
    .split("")
    .filter((c) => {
      const code = c.charCodeAt(0);
      return code >= 32 || c === "\n" || c === "\t";
    })
    .join("");

  // Limit length
  return limitLength(cleaned, MAX_PROMPT_LENGTH);
}
