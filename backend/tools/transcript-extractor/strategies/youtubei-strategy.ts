/**
 * YouTubei.js Transcript Extraction Strategy
 *
 * Secondary strategy using the youtubei.js library (Innertube API).
 * Fast and reliable for most videos with captions, but memory-intensive.
 *
 * Features:
 * - IPv6 rotation support for rate limiting avoidance
 * - Caption track priority selection
 * - Multiple response format handling
 */

import { Agent, fetch as undiciFetch } from "undici";
import {
    STRATEGY_TIMEOUTS,
    TranscriptErrorType,
    TranscriptOptions,
    TranscriptOutcome,
    TranscriptSegment,
} from "../types/index.js";
import { withTimeout } from "../utils/circuit-breaker.js";
import { selectRandomIPv6Address } from "../utils/ipv6-selector.js";
import { logger } from "../utils/logger.js";
import {
    deduplicateSegments,
    parseXmlCaptions,
    segmentsToTranscript,
} from "../utils/vtt-parser.js";
import { classifyTranscriptError, TranscriptStrategy } from "./base.js";

interface YoutubeISegment {
  start_ms?: number | string;
  snippet?: {
    text?: string;
  };
  text?: string;
}

interface CaptionTrack {
  language_code?: string;
  kind?: string;
  base_url?: string;
}

export class YoutubeiStrategy extends TranscriptStrategy {
  constructor() {
    super({
      name: "youtubei",
      timeoutMs: STRATEGY_TIMEOUTS.youtubei,
      enabled: true,
      priority: 2, // Second priority
    });
  }

  async isAvailable(): Promise<boolean> {
    return true; // youtubei.js is a dependency
  }

  async extract(
    videoId: string,
    options: TranscriptOptions
  ): Promise<TranscriptOutcome> {
    const startTime = Date.now();
    const timeoutMs = options.timeoutMs ?? this.config.timeoutMs;
    const includeTimestamps = options.includeTimestamps ?? true;

    try {
      logger.info(
        { videoId, strategy: this.config.name },
        "Starting extraction"
      );

      // Dynamic import to avoid loading if not needed
      const { Innertube } = await import("youtubei.js");

      // Build options with IPv6 rotation if available (using shared utility)
      const innertubeOptions: Record<string, unknown> = {};
      const ipv6Address = selectRandomIPv6Address();
      if (ipv6Address) {
        innertubeOptions.fetch = this.createBoundFetch(ipv6Address);
        logger.debug({ ipv6Address }, "Using IPv6 rotation");
      }

      const youtube = await Innertube.create(innertubeOptions);
      const info = await youtube.getInfo(videoId);

      // Check for live/upcoming
      const liveCheck = this.checkLiveStatus(info);
      if (liveCheck) return liveCheck;

      // Check playability
      const playabilityCheck = this.checkPlayability(info);
      if (playabilityCheck) return playabilityCheck;

      // Extract transcript
      const transcript = await this.tryExtractTranscript(
        info,
        videoId,
        options,
        timeoutMs
      );

      const segments = this.parseTranscriptToSegments(transcript);
      const deduplicated = deduplicateSegments(segments);
      const formattedTranscript = segmentsToTranscript(
        deduplicated,
        includeTimestamps
      );

      const durationMs = Date.now() - startTime;
      logger.info(
        {
          videoId,
          strategy: this.config.name,
          durationMs,
          length: formattedTranscript.length,
          ipv6Used: !!ipv6Address,
        },
        "Extraction successful"
      );

      return {
        success: true,
        transcript: formattedTranscript,
        source: "youtubei",
        segmentCount: deduplicated.length,
      };
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      const errorType = classifyTranscriptError(errorMessage);

      logger.warn(
        { videoId, strategy: this.config.name, error: errorMessage, errorType },
        "Extraction failed"
      );

      return {
        success: false,
        error: errorMessage,
        errorType,
        source: "youtubei",
      };
    }
  }



  /**
   * Create fetch function bound to specific local address
   */
  private createBoundFetch(localAddress: string) {
    const agent = new Agent({
      connect: {
        localAddress,
      },
    });

    return (url: string | URL, options: RequestInit = {}) => {
      return undiciFetch(url.toString(), {
        ...options,
        dispatcher: agent,
      } as Parameters<typeof undiciFetch>[1]);
    };
  }

  private checkLiveStatus(info: unknown): TranscriptOutcome | null {
    const typedInfo = info as {
      basic_info?: { is_live?: boolean; is_upcoming?: boolean };
    };

    if (typedInfo?.basic_info?.is_live) {
      return {
        success: false,
        error: "Cannot get transcript for live video",
        errorType: TranscriptErrorType.VIDEO_LIVE,
        source: "youtubei",
      };
    }

    if (typedInfo?.basic_info?.is_upcoming) {
      return {
        success: false,
        error: "Cannot get transcript for upcoming video",
        errorType: TranscriptErrorType.VIDEO_LIVE,
        source: "youtubei",
      };
    }

    return null;
  }

  private checkPlayability(info: unknown): TranscriptOutcome | null {
    const typedInfo = info as {
      playability_status?: {
        status?: string;
        reason?: string;
      };
    };

    const status = typedInfo?.playability_status?.status;
    const reason = typedInfo?.playability_status?.reason || "";

    if (status === "LOGIN_REQUIRED") {
      return {
        success: false,
        error: reason || "Video requires login (age-restricted)",
        errorType: TranscriptErrorType.AGE_RESTRICTED,
        source: "youtubei",
      };
    }

    if (status === "UNPLAYABLE" || status === "ERROR") {
      if (reason.toLowerCase().includes("private")) {
        return {
          success: false,
          error: reason || "Video is private",
          errorType: TranscriptErrorType.VIDEO_PRIVATE,
          source: "youtubei",
        };
      }
      return {
        success: false,
        error: reason || "Video is unavailable",
        errorType: TranscriptErrorType.VIDEO_UNAVAILABLE,
        source: "youtubei",
      };
    }

    return null;
  }

  private async tryExtractTranscript(
    info: unknown,
    _videoId: string,
    options: TranscriptOptions,
    timeoutMs: number
  ): Promise<string> {
    const typedInfo = info as {
      getTranscript?: () => Promise<unknown>;
      captions?: {
        caption_tracks?: CaptionTrack[];
      };
    };

    // Try getTranscript() method first
    if (typeof typedInfo.getTranscript === "function") {
      try {
        const transcriptData = await withTimeout(
          typedInfo.getTranscript(),
          timeoutMs,
          "Get transcript"
        );
        const parsed = this.parseTranscriptData(transcriptData);
        if (parsed) return parsed;
      } catch (error) {
        logger.debug(
          { error: String(error) },
          "getTranscript failed, trying caption tracks"
        );
      }
    }

    // Fallback: try fetching caption tracks directly
    const captionTracks = typedInfo?.captions?.caption_tracks;
    if (!captionTracks || captionTracks.length === 0) {
      throw new Error("No captions available for this video");
    }

    // Sort by preference
    const preferredLanguages = options.preferredLanguages || ["en", "*"];
    const sortedTracks = this.sortCaptionTracks(captionTracks, preferredLanguages);

    for (const track of sortedTracks) {
      if (!track.base_url) continue;

      try {
        const response = await withTimeout(
          fetch(track.base_url),
          15000,
          "Fetch caption track"
        );
        const xml = await response.text();
        const segments = parseXmlCaptions(xml);
        if (segments.length > 0) {
          return segmentsToTranscript(segments, true);
        }
      } catch {
        // Try next track
      }
    }

    throw new Error("Failed to fetch any caption track");
  }

  private sortCaptionTracks(
    tracks: CaptionTrack[],
    preferredLanguages: string[]
  ): CaptionTrack[] {
    return [...tracks].sort((a, b) => {
      const aLang = a.language_code || "";
      const bLang = b.language_code || "";

      // Prefer manual over auto-generated
      const aAuto = a.kind === "asr" ? 1 : 0;
      const bAuto = b.kind === "asr" ? 1 : 0;
      if (aAuto !== bAuto) return aAuto - bAuto;

      // Prefer earlier in preference list
      const aIdx = preferredLanguages.findIndex(
        (l) => l === "*" || aLang.startsWith(l)
      );
      const bIdx = preferredLanguages.findIndex(
        (l) => l === "*" || bLang.startsWith(l)
      );

      const aScore = aIdx === -1 ? 999 : aIdx;
      const bScore = bIdx === -1 ? 999 : bIdx;

      return aScore - bScore;
    });
  }

  private parseTranscriptData(transcriptData: unknown): string | null {
    const data = transcriptData as {
      transcript?: {
        content?: {
          body?: {
            initial_segments?: YoutubeISegment[];
          };
        };
      };
    };

    const segments = data?.transcript?.content?.body?.initial_segments;
    if (!Array.isArray(segments) || segments.length === 0) {
      return null;
    }

    const lines: string[] = [];
    for (const seg of segments) {
      const startMs = Number(seg.start_ms ?? 0);
      const text = seg.snippet?.text || seg.text || "";
      if (text) {
        const ts = this.formatTimestamp(startMs);
        lines.push(`[${ts}] ${text.trim()}`);
      }
    }

    return lines.join("\n");
  }

  private parseTranscriptToSegments(transcript: string): TranscriptSegment[] {
    const segments: TranscriptSegment[] = [];
    const lines = transcript.split("\n");

    for (const line of lines) {
      const match = line.match(/^\[(\d{2}):(\d{2}):(\d{2})\]\s*(.+)$/);
      if (match) {
        const hours = parseInt(match[1], 10);
        const minutes = parseInt(match[2], 10);
        const seconds = parseInt(match[3], 10);
        const startMs = (hours * 3600 + minutes * 60 + seconds) * 1000;
        segments.push({ startMs, text: match[4] });
      }
    }

    return segments;
  }

  private formatTimestamp(ms: number): string {
    const totalSeconds = Math.max(0, Math.floor(ms / 1000));
    const hours = String(Math.floor(totalSeconds / 3600)).padStart(2, "0");
    const minutes = String(Math.floor((totalSeconds % 3600) / 60)).padStart(
      2,
      "0"
    );
    const seconds = String(totalSeconds % 60).padStart(2, "0");
    return `${hours}:${minutes}:${seconds}`;
  }
}
