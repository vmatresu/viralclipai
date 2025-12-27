/**
 * yt-dlp Transcript Extraction Strategy
 *
 * Fallback strategy using yt-dlp CLI tool.
 * More robust for edge cases but slower due to external process execution.
 *
 * Features:
 * - Cookie support for authenticated access
 * - IPv6 rotation support
 * - Multiple output format parsing
 */

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readdir, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import {
    STRATEGY_TIMEOUTS,
    TranscriptErrorType,
    TranscriptOptions,
    TranscriptOutcome,
} from "../types/index.js";
import { Config } from "../utils/config.js";
import { selectRandomIPv6Address } from "../utils/ipv6-selector.js";
import { logger } from "../utils/logger.js";
import { parseVttToTranscript } from "../utils/vtt-parser.js";
import { classifyTranscriptError, TranscriptStrategy } from "./base.js";

export class YtdlpStrategy extends TranscriptStrategy {
  private ytdlpPath: string;
  private cookiesPath?: string;

  constructor(options?: { ytdlpPath?: string; cookiesPath?: string }) {
    super({
      name: "ytdlp",
      timeoutMs: STRATEGY_TIMEOUTS.ytdlp,
      enabled: true,
      priority: 3, // Third priority
    });

    this.ytdlpPath = options?.ytdlpPath || Config.ytdlpPath || "yt-dlp";
    this.cookiesPath = options?.cookiesPath || Config.ytdlpCookiesPath;
  }

  async isAvailable(): Promise<boolean> {
    try {
      const result = await this.runCommand([this.ytdlpPath, "--version"], 5000);
      if (result.exitCode === 0) {
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }

  async extract(
    videoId: string,
    options: TranscriptOptions
  ): Promise<TranscriptOutcome> {
    const startTime = Date.now();
    const timeoutMs = options.timeoutMs ?? this.config.timeoutMs;
    const includeTimestamps = options.includeTimestamps ?? true;
    const workDir = options.workDir || Config.workDir;

    // Create unique extraction directory
    const extractionDir = join(workDir, `ytdlp-${videoId}-${Date.now()}`);

    try {
      logger.info(
        { videoId, strategy: this.config.name },
        "Starting extraction"
      );

      // Ensure extraction directory exists
      await mkdir(extractionDir, { recursive: true });

      // Build yt-dlp arguments
      const args = this.buildArgs(
        videoId,
        extractionDir,
        options,
        this.cookiesPath
      );

      // Run yt-dlp
      const result = await this.runCommand(
        [this.ytdlpPath, ...args],
        timeoutMs
      );

      if (result.exitCode !== 0) {
        const parsed = this.parseYtdlpError(result.stderr);
        await this.cleanup(extractionDir);
        return {
          success: false,
          error: parsed.message,
          errorType: parsed.type,
          source: "ytdlp",
        };
      }

      // Find and parse VTT file
      const transcript = await this.findAndParseVtt(
        extractionDir,
        includeTimestamps
      );

      if (!transcript) {
        await this.cleanup(extractionDir);
        return {
          success: false,
          error: "No transcript file downloaded. Video may not have captions.",
          errorType: TranscriptErrorType.NO_CAPTIONS,
          source: "ytdlp",
        };
      }

      await this.cleanup(extractionDir);

      const durationMs = Date.now() - startTime;
      logger.info(
        {
          videoId,
          strategy: this.config.name,
          durationMs,
          length: transcript.length,
        },
        "Extraction successful"
      );

      return {
        success: true,
        transcript,
        source: "ytdlp",
      };
    } catch (error) {
      await this.cleanup(extractionDir).catch(() => {});

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
        source: "ytdlp",
      };
    }
  }

  private buildArgs(
    videoId: string,
    extractionDir: string,
    _options: TranscriptOptions,
    cookiesPath?: string
  ): string[] {
    const outputTemplate = join(extractionDir, "%(id)s");

    const args = [
      "--write-auto-sub",
      "--write-sub",
      "--sub-lang",
      "en,en-US,en-GB",
      "--skip-download",
      "--sub-format",
      "vtt",
      "--output",
      outputTemplate,
    ];

    // Add cookies if available
    if (cookiesPath && existsSync(cookiesPath)) {
      args.push("--cookies", cookiesPath);
    }

    // IPv6 rotation using shared utility
    const ipv6Address = selectRandomIPv6Address();
    if (ipv6Address) {
      args.push("--source-address", ipv6Address);
      logger.debug({ ipv6Address }, "Using IPv6 rotation");
    }

    // Add video URL
    args.push(`https://www.youtube.com/watch?v=${videoId}`);

    return args;
  }



  private runCommand(
    command: string[],
    timeoutMs: number
  ): Promise<{ exitCode: number; stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      const [cmd, ...args] = command;
      const proc = spawn(cmd, args, {
        stdio: ["pipe", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";

      proc.stdout?.on("data", (data) => {
        stdout += data.toString();
      });

      proc.stderr?.on("data", (data) => {
        stderr += data.toString();
      });

      const timeout = setTimeout(() => {
        proc.kill("SIGKILL");
        reject(new Error(`yt-dlp timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      proc.on("close", (code) => {
        clearTimeout(timeout);
        resolve({
          exitCode: code ?? 1,
          stdout,
          stderr,
        });
      });

      proc.on("error", (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }

  private async findAndParseVtt(
    extractionDir: string,
    includeTimestamps: boolean
  ): Promise<string | null> {
    try {
      const files = await readdir(extractionDir);
      const vttFiles = files
        .filter((f) => f.endsWith(".vtt"))
        .sort((a, b) => {
          // Prefer English subtitles
          const aEn = a.includes(".en") ? 0 : 1;
          const bEn = b.includes(".en") ? 0 : 1;
          return aEn - bEn;
        });

      if (vttFiles.length === 0) return null;

      const vttPath = join(extractionDir, vttFiles[0]);
      const content = await readFile(vttPath, "utf8");

      return parseVttToTranscript(content, includeTimestamps);
    } catch {
      return null;
    }
  }

  private async cleanup(extractionDir: string): Promise<void> {
    try {
      await rm(extractionDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  }

  private parseYtdlpError(stderr: string): {
    message: string;
    type: TranscriptErrorType;
  } {
    const lower = stderr.toLowerCase();

    // Check for specific error patterns
    if (
      lower.includes("video unavailable") ||
      lower.includes("this video is unavailable")
    ) {
      return {
        message: "Video is unavailable",
        type: TranscriptErrorType.VIDEO_UNAVAILABLE,
      };
    }

    if (lower.includes("private video") || lower.includes("video is private")) {
      return {
        message: "Video is private",
        type: TranscriptErrorType.VIDEO_PRIVATE,
      };
    }

    if (
      lower.includes("sign in") ||
      lower.includes("age-restricted") ||
      lower.includes("age restricted") ||
      lower.includes("confirm your age")
    ) {
      return {
        message: "Video is age-restricted",
        type: TranscriptErrorType.AGE_RESTRICTED,
      };
    }

    if (lower.includes("live stream") || lower.includes("is live")) {
      return {
        message: "Cannot get transcript for live video",
        type: TranscriptErrorType.VIDEO_LIVE,
      };
    }

    if (
      lower.includes("no subtitles") ||
      lower.includes("no captions") ||
      lower.includes("subtitles are disabled")
    ) {
      return {
        message: "No captions available for this video",
        type: TranscriptErrorType.NO_CAPTIONS,
      };
    }

    if (lower.includes("rate limit") || lower.includes("too many requests")) {
      return {
        message: "Rate limited by YouTube",
        type: TranscriptErrorType.RATE_LIMITED,
      };
    }

    if (lower.includes("timed out") || lower.includes("timeout")) {
      return {
        message: "Request timed out",
        type: TranscriptErrorType.TIMEOUT,
      };
    }

    // Extract first line of error for generic message
    const firstLine = stderr.split("\n").find((l) => l.trim()) || stderr;
    return {
      message: firstLine.slice(0, 200),
      type: TranscriptErrorType.UNKNOWN,
    };
  }
}
