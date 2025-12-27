/**
 * Video ID Utilities
 *
 * Provides validation and branded type for YouTube video IDs.
 * Centralizes video ID handling for type safety and security.
 *
 * @module utils/video-id
 */

/**
 * YouTube video ID regex pattern (11 alphanumeric + _ -)
 * This is the ONLY place this pattern should be defined.
 */
const VIDEO_ID_REGEX = /^[A-Za-z0-9_-]{11}$/;

/**
 * Branded type for validated YouTube video IDs
 *
 * This prevents passing arbitrary strings where video IDs are expected.
 * Use `validateVideoId()` to create a VideoId from untrusted input.
 */
export type VideoId = string & { readonly __brand: "VideoId" };

/**
 * Validate and create a VideoId from a string
 *
 * @param input - String to validate
 * @returns VideoId if valid, null otherwise
 */
export function validateVideoId(input: string): VideoId | null {
  const trimmed = input.trim();

  if (VIDEO_ID_REGEX.test(trimmed)) {
    return trimmed as VideoId;
  }

  return null;
}

/**
 * Check if a string is a valid video ID (without branding)
 *
 * Use this for quick validation when you don't need the branded type.
 */
export function isValidVideoId(input: string): boolean {
  return VIDEO_ID_REGEX.test(input.trim());
}

/**
 * Assert that a string is a valid video ID
 *
 * @throws Error if input is not a valid video ID
 */
export function assertVideoId(input: string): asserts input is VideoId {
  if (!isValidVideoId(input)) {
    throw new Error(`Invalid YouTube video ID: ${input.slice(0, 20)}`);
  }
}
