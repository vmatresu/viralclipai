/**
 * YouTube URL Parser
 *
 * Extracts video IDs from various YouTube URL formats.
 * Supports: youtube.com, youtu.be, shorts, embeds, and raw IDs.
 *
 * @module utils/youtube-url-parser
 */

import { isValidVideoId } from "./video-id.js";

/**
 * Extract video ID from a YouTube URL or return if already an ID
 *
 * Supported formats:
 * - https://www.youtube.com/watch?v=VIDEO_ID
 * - https://youtu.be/VIDEO_ID
 * - https://www.youtube.com/shorts/VIDEO_ID
 * - https://www.youtube.com/embed/VIDEO_ID
 * - https://www.youtube.com/v/VIDEO_ID
 * - Raw 11-character video ID
 *
 * @param input - URL or video ID
 * @returns Video ID or null if invalid
 */
export function extractVideoIdSimple(input: string): string | null {
  const trimmed = input.trim();

  // Check if it's already a valid video ID
  if (isValidVideoId(trimmed)) {
    return trimmed;
  }

  try {
    const url = new URL(trimmed);
    const host = url.hostname.toLowerCase().replace("www.", "");

    // youtu.be short links
    if (host === "youtu.be") {
      const id = url.pathname.slice(1).split("/")[0];
      return isValidVideoId(id) ? id : null;
    }

    // youtube.com variants
    if (host === "youtube.com" || host === "m.youtube.com") {
      // Standard watch URLs
      const vParam = url.searchParams.get("v");
      if (vParam && isValidVideoId(vParam)) {
        return vParam;
      }

      // Shorts, embed, and v URLs
      const pathParts = url.pathname.split("/").filter(Boolean);

      for (const prefix of ["shorts", "embed", "v"]) {
        const index = pathParts.indexOf(prefix);
        if (index !== -1 && pathParts[index + 1]) {
          const id = pathParts[index + 1].split("?")[0];
          if (isValidVideoId(id)) {
            return id;
          }
        }
      }
    }
  } catch {
    // Not a valid URL
  }

  return null;
}
