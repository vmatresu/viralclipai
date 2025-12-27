/**
 * VTT (WebVTT) and XML Caption Parser
 *
 * Parses WebVTT subtitle files and YouTube XML captions into transcript text.
 * Handles deduplication of rolling captions and proper timestamp formatting.
 */

import type { TranscriptSegment } from "../types/index.js";

/**
 * Format milliseconds to [HH:MM:SS] timestamp
 */
export function formatTimestamp(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = String(Math.floor(totalSeconds / 3600)).padStart(2, "0");
  const minutes = String(Math.floor((totalSeconds % 3600) / 60)).padStart(
    2,
    "0"
  );
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  return `${hours}:${minutes}:${seconds}`;
}

/**
 * Parse VTT timestamp to milliseconds
 * Format: HH:MM:SS.mmm or MM:SS.mmm
 */
export function parseVttTimestamp(timestamp: string): number {
  const parts = timestamp.trim().split(":");
  let hours = 0;
  let minutes = 0;
  let seconds = 0;

  if (parts.length === 3) {
    hours = Number.parseInt(parts[0], 10);
    minutes = Number.parseInt(parts[1], 10);
    seconds = Number.parseFloat(parts[2]);
  } else if (parts.length === 2) {
    minutes = Number.parseInt(parts[0], 10);
    seconds = Number.parseFloat(parts[1]);
  }

  return (hours * 3600 + minutes * 60 + seconds) * 1000;
}

/**
 * Strip VTT formatting tags from text
 */
export function stripVttTags(text: string): string {
  return (
    text
      // Remove voice tags <v ...>
      .replace(/<v[^>]*>/g, "")
      // Remove class tags <c>
      .replace(/<c[^>]*>/g, "")
      .replace(/<\/c>/g, "")
      // Remove other tags
      .replace(/<[^>]+>/g, "")
      // Decode HTML entities
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&nbsp;/g, " ")
      // Normalize whitespace
      .replace(/\s+/g, " ")
      .trim()
  );
}

/**
 * Check if a line is a VTT cue identifier (numeric or alphanumeric)
 */
function isCueIdentifier(line: string): boolean {
  // Skip pure numeric lines (cue IDs)
  if (/^\d+$/.test(line.trim())) {
    return true;
  }
  // Skip lines that look like cue identifiers (e.g., "1", "cue-1")
  if (/^[\w-]+$/.test(line.trim()) && line.trim().length < 20) {
    return true;
  }
  return false;
}

/**
 * Parse VTT content into transcript segments
 */
export function parseVttContent(vttContent: string): TranscriptSegment[] {
  const segments: TranscriptSegment[] = [];
  const lines = vttContent.split("\n");

  let currentSegment: Partial<TranscriptSegment> | null = null;
  let textBuffer: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();

    // Skip empty lines, WEBVTT header, NOTE lines, and STYLE blocks
    if (
      !trimmed ||
      trimmed === "WEBVTT" ||
      trimmed.startsWith("NOTE") ||
      trimmed.startsWith("STYLE") ||
      trimmed.startsWith("Kind:") ||
      trimmed.startsWith("Language:")
    ) {
      // If we have accumulated text and a segment, save it
      if (currentSegment?.startMs !== undefined && textBuffer.length > 0) {
        const text = textBuffer.join(" ").trim();
        if (text) {
          segments.push({
            startMs: currentSegment.startMs,
            endMs: currentSegment.endMs,
            text: stripVttTags(text),
          });
        }
        currentSegment = null;
        textBuffer = [];
      }
      continue;
    }

    // Check for timestamp line: 00:00:00.000 --> 00:00:05.000
    const timestampMatch = trimmed.match(
      /^(\d{1,2}:\d{2}:\d{2}[.,]\d{3})\s*-->\s*(\d{1,2}:\d{2}:\d{2}[.,]\d{3})/
    );

    if (timestampMatch) {
      // Save previous segment if exists
      if (currentSegment?.startMs !== undefined && textBuffer.length > 0) {
        const text = textBuffer.join(" ").trim();
        if (text) {
          segments.push({
            startMs: currentSegment.startMs,
            endMs: currentSegment.endMs,
            text: stripVttTags(text),
          });
        }
      }

      // Start new segment
      currentSegment = {
        startMs: parseVttTimestamp(timestampMatch[1].replace(",", ".")),
        endMs: parseVttTimestamp(timestampMatch[2].replace(",", ".")),
      };
      textBuffer = [];
      continue;
    }

    // Skip cue identifiers
    if (isCueIdentifier(trimmed)) {
      continue;
    }

    // Accumulate text
    if (currentSegment) {
      textBuffer.push(trimmed);
    }
  }

  // Don't forget the last segment
  if (currentSegment?.startMs !== undefined && textBuffer.length > 0) {
    const text = textBuffer.join(" ").trim();
    if (text) {
      segments.push({
        startMs: currentSegment.startMs,
        endMs: currentSegment.endMs,
        text: stripVttTags(text),
      });
    }
  }

  return segments;
}

/**
 * Deduplicate rolling captions (YouTube auto-captions often repeat text)
 */
export function deduplicateSegments(
  segments: TranscriptSegment[]
): TranscriptSegment[] {
  if (segments.length === 0) {
    return [];
  }

  const deduplicated: TranscriptSegment[] = [];
  let lastText = "";

  for (const segment of segments) {
    const text = segment.text.trim();

    // Skip if text is identical to previous
    if (text === lastText) {
      continue;
    }

    // Skip if current text is a substring of the last (rolling caption overlap)
    if (lastText.endsWith(text) || lastText.startsWith(text)) {
      continue;
    }

    // Skip if last text is a substring of current (we'll use the longer one)
    if (
      (text.endsWith(lastText) || text.startsWith(lastText)) &&
      deduplicated.length > 0
    ) {
      // Replace last entry with this longer one
      deduplicated[deduplicated.length - 1] = segment;
      lastText = text;
      continue;
    }

    deduplicated.push(segment);
    lastText = text;
  }

  return deduplicated;
}

/**
 * Convert segments to formatted transcript with timestamps
 */
export function segmentsToTranscript(
  segments: TranscriptSegment[],
  includeTimestamps: boolean = true
): string {
  const lines = segments.map((segment) => {
    if (includeTimestamps) {
      return `[${formatTimestamp(segment.startMs)}] ${segment.text}`;
    }
    return segment.text;
  });

  return lines.join("\n");
}

/**
 * Parse VTT file content into formatted transcript
 */
export function parseVttToTranscript(
  vttContent: string,
  includeTimestamps: boolean = true
): string {
  const segments = parseVttContent(vttContent);
  const deduplicated = deduplicateSegments(segments);
  return segmentsToTranscript(deduplicated, includeTimestamps);
}

/**
 * Parse XML caption format (YouTube's timedtext endpoint)
 */
export function parseXmlCaptions(xml: string): TranscriptSegment[] {
  const segments: TranscriptSegment[] = [];
  const textRegex =
    /<text[^>]*start="([^"]*)"[^>]*(?:dur="([^"]*)")?[^>]*>([^<]*)<\/text>/g;

  let match;
  while ((match = textRegex.exec(xml)) !== null) {
    const startMs = Number.parseFloat(match[1]) * 1000;
    const durMs = match[2] ? Number.parseFloat(match[2]) * 1000 : undefined;
    const text = match[3]
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/\n/g, " ")
      .trim();

    if (text) {
      segments.push({
        startMs,
        endMs: durMs ? startMs + durMs : undefined,
        text,
      });
    }
  }

  return segments;
}
