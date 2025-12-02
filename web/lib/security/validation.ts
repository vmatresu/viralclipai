/**
 * Security validation utilities
 * Provides input validation and sanitization functions
 */

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
  return !!(
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
