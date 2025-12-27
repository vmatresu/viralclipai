/**
 * YouTube Data API v3 Transcript Extraction Strategy
 *
 * Final fallback strategy using the official YouTube Data API.
 * Requires an API key but is the most reliable for official captions.
 */

import { google, youtube_v3 } from "googleapis";
import {
    STRATEGY_TIMEOUTS,
    TranscriptErrorType,
    TranscriptOptions,
    TranscriptOutcome,
} from "../types/index.js";
import { withTimeout } from "../utils/circuit-breaker.js";
import { Config } from "../utils/config.js";
import { logger } from "../utils/logger.js";
import {
    deduplicateSegments,
    parseXmlCaptions,
    segmentsToTranscript,
} from "../utils/vtt-parser.js";
import { classifyTranscriptError, TranscriptStrategy } from "./base.js";

export class YouTubeApiStrategy extends TranscriptStrategy {
  private youtube: youtube_v3.Youtube | null = null;

  constructor() {
    super({
      name: "youtube-api",
      timeoutMs: STRATEGY_TIMEOUTS.youtubeApi,
      enabled: Boolean(Config.youtubeApiKey),
      priority: 4, // Fourth priority
    });
  }

  async isAvailable(): Promise<boolean> {
    return Boolean(Config.youtubeApiKey);
  }

  private getYouTubeClient(): youtube_v3.Youtube {
    if (!this.youtube) {
      this.youtube = google.youtube({
        version: "v3",
        auth: Config.youtubeApiKey,
      });
    }
    return this.youtube;
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

      const youtube = this.getYouTubeClient();

      // Get video details to check status
      const videoResponse = await withTimeout(
        youtube.videos.list({
          part: ["status", "contentDetails"],
          id: [videoId],
        }),
        timeoutMs,
        "Fetch video details"
      );

      const video = videoResponse.data.items?.[0];
      if (!video) {
        return {
          success: false,
          error: "Video not found",
          errorType: TranscriptErrorType.VIDEO_UNAVAILABLE,
          source: "youtube-api",
        };
      }

      // Check video status
      const statusCheck = this.checkVideoStatus(video);
      if (statusCheck) return statusCheck;

      // Get captions list
      const captionsResponse = await withTimeout(
        youtube.captions.list({
          part: ["snippet"],
          videoId,
        }),
        timeoutMs,
        "Fetch captions list"
      );

      const captions = captionsResponse.data.items || [];
      if (captions.length === 0) {
        return {
          success: false,
          error: "No captions available for this video",
          errorType: TranscriptErrorType.NO_CAPTIONS,
          source: "youtube-api",
        };
      }

      // Sort captions by preference
      const preferredLanguages = options.preferredLanguages || ["en", "*"];
      const sortedCaptions = this.sortCaptions(captions, preferredLanguages);

      // Try to fetch each caption track
      for (const caption of sortedCaptions) {
        const language = caption.snippet?.language || "en";
        const transcript = await this.fetchCaptionViaTimedText(
          videoId,
          language,
          includeTimestamps,
          timeoutMs
        );

        if (transcript) {
          const durationMs = Date.now() - startTime;
          logger.info(
            {
              videoId,
              strategy: this.config.name,
              durationMs,
              length: transcript.length,
              language,
            },
            "Extraction successful"
          );

          return {
            success: true,
            transcript,
            source: "youtube-api",
            language,
          };
        }
      }

      return {
        success: false,
        error: "Failed to download any caption tracks",
        errorType: TranscriptErrorType.NO_CAPTIONS,
        source: "youtube-api",
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
        source: "youtube-api",
      };
    }
  }

  private checkVideoStatus(
    video: youtube_v3.Schema$Video
  ): TranscriptOutcome | null {
    const privacyStatus = video.status?.privacyStatus;
    const uploadStatus = video.status?.uploadStatus;

    if (privacyStatus === "private") {
      return {
        success: false,
        error: "Video is private",
        errorType: TranscriptErrorType.VIDEO_PRIVATE,
        source: "youtube-api",
      };
    }

    if (uploadStatus === "rejected" || uploadStatus === "failed") {
      return {
        success: false,
        error: "Video is unavailable",
        errorType: TranscriptErrorType.VIDEO_UNAVAILABLE,
        source: "youtube-api",
      };
    }

    // Check content details for live broadcasts
    const contentDetails = video.contentDetails;
    if (contentDetails?.duration === "P0D") {
      // Live video
      return {
        success: false,
        error: "Cannot get transcript for live video",
        errorType: TranscriptErrorType.VIDEO_LIVE,
        source: "youtube-api",
      };
    }

    return null;
  }

  private sortCaptions(
    captions: youtube_v3.Schema$Caption[],
    preferredLanguages: string[]
  ): youtube_v3.Schema$Caption[] {
    return [...captions].sort((a, b) => {
      const aLang = a.snippet?.language || "";
      const bLang = b.snippet?.language || "";

      // Prefer manual over auto-generated
      const aAuto = a.snippet?.trackKind === "ASR" ? 1 : 0;
      const bAuto = b.snippet?.trackKind === "ASR" ? 1 : 0;
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

  /**
   * Fetch caption via YouTube's timedtext endpoint
   * This is a publicly available endpoint that doesn't require OAuth
   */
  private async fetchCaptionViaTimedText(
    videoId: string,
    language: string,
    includeTimestamps: boolean,
    timeoutMs: number
  ): Promise<string | null> {
    try {
      const url = `https://www.youtube.com/api/timedtext?v=${videoId}&lang=${language}`;

      const response = await withTimeout(
        fetch(url),
        timeoutMs,
        "Fetch timedtext"
      );

      if (!response.ok) return null;

      const xml = await response.text();
      if (!xml || xml.trim().length < 10) return null;

      const segments = parseXmlCaptions(xml);
      if (segments.length === 0) return null;

      const deduplicated = deduplicateSegments(segments);
      return segmentsToTranscript(deduplicated, includeTimestamps);
    } catch {
      return null;
    }
  }
}
