/**
 * SRT (SubRip) Subtitle Parser
 *
 * Parses SRT subtitle files into transcript segments.
 * Complements the VTT parser for handling different subtitle formats.
 */

import type { TranscriptSegment } from "../types/index.js";

/**
 * Regex pattern for SRT timestamp line: 00:00:00,000 --> 00:00:05,000
 */
const SRT_TIMESTAMP_REGEX =
  /(\d{1,2}):(\d{2}):(\d{1,2})[,.](\d{3})\s*-->\s*(\d{1,2}):(\d{2}):(\d{1,2})[,.](\d{3})/;

/**
 * Parse SRT timestamp components to milliseconds
 */
function parseTimestampToMs(
  hours: string,
  minutes: string,
  seconds: string,
  milliseconds: string
): number {
  return (
    Number.parseInt(hours, 10) * 3600000 +
    Number.parseInt(minutes, 10) * 60000 +
    Number.parseInt(seconds, 10) * 1000 +
    Number.parseInt(milliseconds, 10)
  );
}

/**
 * Strip HTML-style tags and normalize whitespace
 */
function sanitizeText(text: string): string {
  return text
    .replace(/<[^>]+>/g, "") // Remove HTML tags
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ") // Normalize whitespace
    .trim();
}

/**
 * Parse SRT content into transcript segments
 *
 * @param srt - Raw SRT file content
 * @returns Array of transcript segments with timing and text
 */
export function parseSrtContent(srt: string): TranscriptSegment[] {
  const segments: TranscriptSegment[] = [];

  // Split into subtitle blocks (separated by blank lines)
  const blocks = srt.split(/\n\s*\n/);

  for (const block of blocks) {
    const trimmedBlock = block.trim();
    if (trimmedBlock.length === 0) {
      continue;
    }

    const lines = trimmedBlock.split("\n");

    // Find the timestamp line
    const timestampLineIndex = lines.findIndex((line) =>
      line.includes("-->")
    );

    if (timestampLineIndex === -1) {
      continue;
    }

    const timestampMatch = lines[timestampLineIndex].match(SRT_TIMESTAMP_REGEX);
    if (!timestampMatch) {
      continue;
    }

    const startMs = parseTimestampToMs(
      timestampMatch[1],
      timestampMatch[2],
      timestampMatch[3],
      timestampMatch[4]
    );

    const endMs = parseTimestampToMs(
      timestampMatch[5],
      timestampMatch[6],
      timestampMatch[7],
      timestampMatch[8]
    );

    // Get text lines (everything after timestamp line)
    const textLines = lines.slice(timestampLineIndex + 1);
    const text = sanitizeText(textLines.join(" "));

    if (text) {
      segments.push({ startMs, endMs, text });
    }
  }

  return segments;
}

/**
 * Format milliseconds to timestamp string
 * Uses compact format without leading zeros for hours when 0
 */
function formatTimestampCompact(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/**
 * Parse SRT content directly to transcript string
 *
 * @param srt - Raw SRT file content
 * @param includeTimestamps - Whether to include [HH:MM:SS] timestamps
 * @returns Formatted transcript text
 */
export function parseSrtToTranscript(
  srt: string,
  includeTimestamps: boolean = true
): string {
  const segments = parseSrtContent(srt);

  const lines = segments.map((segment) => {
    if (includeTimestamps) {
      const timestamp = formatTimestampCompact(segment.startMs);
      return `[${timestamp}] ${segment.text}`;
    }
    return segment.text;
  });

  return lines.join("\n");
}
