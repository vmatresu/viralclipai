/**
 * yt-dlp Transcript Extraction Strategy
 *
 * Fallback strategy using yt-dlp CLI tool.
 * More robust for edge cases but slower due to external process execution.
 *
 * Features:
 * - PO Token support via bgutil HTTP provider (centralized token generation)
 * - IPv6 rotation for rate limit avoidance
 * - Cookie authentication for age-restricted content
 * - Rate limiting with browser-like headers
 * - Circuit breaker for PO token provider failures
 */

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, readdir, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";

import {
  getPOTokenService,
  type POTokenService,
  type TokenResult,
} from "../po-token/index.js";
import {
  DEFAULT_TRANSCRIPT_OPTIONS,
  STRATEGY_TIMEOUTS,
  TranscriptErrorType,
  type TranscriptOptions,
  type TranscriptOutcome,
} from "../types/index.js";
import { Config } from "../utils/config.js";
import { selectRandomIPv6Address } from "../utils/ipv6-selector.js";
import { logger } from "../utils/logger.js";
import { parseVttToTranscript } from "../utils/vtt-parser.js";
import { classifyTranscriptError, TranscriptStrategy } from "./base.js";

/**
 * Configuration options for the yt-dlp strategy
 */
export interface YtdlpStrategyOptions {
  /** Path to yt-dlp executable */
  ytdlpPath?: string;
  /** Path to cookies file for age-restricted content */
  cookiesPath?: string;
  /** Whether to use PO Token HTTP provider */
  usePOTokenProvider?: boolean;
  /** Custom PO Token service instance (for testing) */
  poTokenService?: POTokenService;
}

export class YtdlpStrategy extends TranscriptStrategy {
  private readonly ytdlpPath: string;
  private readonly cookiesPath?: string;
  private readonly usePOTokenProvider: boolean;
  private readonly poTokenService: POTokenService | null;
  private ytdlpVersion?: string;

  constructor(options?: YtdlpStrategyOptions) {
    super({
      name: "ytdlp",
      timeoutMs: STRATEGY_TIMEOUTS.ytdlp,
      enabled: true,
      priority: 3,
    });

    this.ytdlpPath = options?.ytdlpPath ?? Config.ytdlpPath ?? "yt-dlp";
    this.cookiesPath = options?.cookiesPath ?? Config.ytdlpCookiesPath;
    this.usePOTokenProvider = options?.usePOTokenProvider ?? true;

    // Get PO Token service (lazily initialized singleton)
    // May be null if service initialization failed
    try {
      this.poTokenService = options?.poTokenService ?? getPOTokenService();
    } catch (error) {
      logger.warn(
        { error: error instanceof Error ? error.message : String(error) },
        "PO Token service unavailable, will use fallback mode"
      );
      this.poTokenService = null;
    }
  }

  async isAvailable(): Promise<boolean> {
    try {
      const result = await this.runCommand([this.ytdlpPath, "--version"], 5000);
      const available = result.exitCode === 0;

      if (available) {
        this.ytdlpVersion = result.stdout.trim();

        const poTokenStatus = this.poTokenService?.getStatus();

        logger.info(
          {
            strategy: this.config.name,
            ytdlpPath: this.ytdlpPath,
            ytdlpVersion: this.ytdlpVersion,
            cookiesPath: this.cookiesPath || "(not configured)",
            cookiesExists: this.cookiesPath ? existsSync(this.cookiesPath) : false,
            poTokenEnabled: this.usePOTokenProvider && !!this.poTokenService,
            poTokenProviderUrl: poTokenStatus?.providerUrl,
            poTokenProviderHealthy: poTokenStatus?.providerHealthy,
          },
          "yt-dlp strategy initialized"
        );
      }

      return available;
    } catch {
      logger.warn({ strategy: this.config.name }, "yt-dlp not available");
      return false;
    }
  }

  async extract(
    videoId: string,
    options: TranscriptOptions
  ): Promise<TranscriptOutcome> {
    const startTime = Date.now();
    const correlationId = randomUUID();
    const timeoutMs = options.timeoutMs ?? this.config.timeoutMs;
    const workDir = options.workDir ?? DEFAULT_TRANSCRIPT_OPTIONS.workDir;
    const includeTimestamps = options.includeTimestamps ?? true;

    // Create unique work directory for this extraction
    const extractionDir = path.join(workDir, `ytdlp-${videoId}-${Date.now()}`);

    // Get rotated IPv6 address for this request
    const rotatedIp = selectRandomIPv6Address();

    // Get PO token from HTTP provider (if available)
    const tokenResult = await this.getPOToken(videoId, correlationId);

    try {
      logger.info(
        {
          videoId,
          correlationId,
          strategy: this.config.name,
          workDir: extractionDir,
          hasIPv6: !!rotatedIp,
          hasPOToken: tokenResult.success,
          poTokenDegraded: tokenResult.degraded,
        },
        "Starting extraction"
      );

      // Ensure work directory exists
      await mkdir(extractionDir, { recursive: true });

      // Handle cookies: copy to temp dir to avoid read-only FS issues
      let activeCookiesPath = this.cookiesPath;
      if (this.cookiesPath && existsSync(this.cookiesPath)) {
        try {
          const tempCookiesPath = path.join(extractionDir, "cookies.txt");
          await copyFile(this.cookiesPath, tempCookiesPath);
          activeCookiesPath = tempCookiesPath;
        } catch (error) {
          logger.warn(
            { error, originalPath: this.cookiesPath },
            "Failed to copy cookies file to temp dir, trying original"
          );
        }
      }

      // Build yt-dlp command arguments
      const args = this.buildArgs(
        videoId,
        extractionDir,
        options,
        activeCookiesPath,
        rotatedIp ?? undefined,
        tokenResult
      );

      logger.debug(
        {
          videoId,
          correlationId,
          strategy: this.config.name,
          hasCookies: !!this.cookiesPath,
          timeoutMs,
          playerClient: tokenResult.metadata?.client ?? "web_creator",
        },
        "Executing yt-dlp command"
      );

      // Run yt-dlp
      const result = await this.runCommand([this.ytdlpPath, ...args], timeoutMs);

      logger.debug(
        {
          videoId,
          correlationId,
          strategy: this.config.name,
          exitCode: result.exitCode,
          stderrPreview: result.stderr.slice(0, 300),
          durationMs: Date.now() - startTime,
        },
        "yt-dlp command completed"
      );

      if (result.exitCode !== 0) {
        const error = this.parseYtdlpError(result.stderr, tokenResult);

        logger.warn(
          {
            videoId,
            correlationId,
            strategy: this.config.name,
            exitCode: result.exitCode,
            errorType: error.type,
            errorMessage: error.message,
            ytdlpVersion: this.ytdlpVersion,
            stderrTail: result.stderr.slice(-500),
          },
          "yt-dlp extraction failed"
        );

        return {
          success: false,
          error: error.message,
          errorType: error.type,
          source: "ytdlp",
        };
      }

      // Find and parse VTT file
      const transcript = await this.findAndParseVtt(extractionDir, includeTimestamps);

      if (!transcript || transcript.length < 10) {
        return {
          success: false,
          error: "No captions found for this video",
          errorType: TranscriptErrorType.NO_CAPTIONS,
          source: "ytdlp",
        };
      }

      const durationMs = Date.now() - startTime;
      logger.info(
        {
          videoId,
          correlationId,
          strategy: this.config.name,
          durationMs,
          length: transcript.length,
          usedPOToken: tokenResult.success,
        },
        "Extraction successful"
      );

      return {
        success: true,
        transcript,
        source: "ytdlp",
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      logger.warn(
        { videoId, correlationId, strategy: this.config.name, error: errorMessage },
        "Extraction failed"
      );

      return {
        success: false,
        error: errorMessage,
        errorType: classifyTranscriptError(errorMessage),
        source: "ytdlp",
      };
    } finally {
      await this.cleanup(extractionDir);
    }
  }

  /**
   * Get PO token from HTTP provider
   */
  private async getPOToken(
    videoId: string,
    correlationId: string
  ): Promise<TokenResult> {
    if (!this.usePOTokenProvider || !this.poTokenService) {
      return {
        success: false,
        error: "PO Token provider not configured",
        degraded: true,
      };
    }

    try {
      return await this.poTokenService.getToken({
        videoId,
        correlationId,
        // Use mweb client for GVS requests as per yt-dlp recommendations
        client: "mweb",
        context: "gvs",
      });
    } catch (error) {
      logger.warn(
        {
          videoId,
          correlationId,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to get PO token, using fallback"
      );

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        degraded: true,
      };
    }
  }

  private buildArgs(
    videoId: string,
    extractionDir: string,
    options: TranscriptOptions,
    cookiesPath?: string,
    sourceIp?: string,
    tokenResult?: TokenResult
  ): string[] {
    const videoUrl = `https://www.youtube.com/watch?v=${videoId}`;
    const outputTemplate = path.join(extractionDir, "%(id)s");

    // Build language spec
    const rawLanguages = options.preferredLanguages ?? ["en", "en-US", "en-GB"];
    const baseLangs = rawLanguages
      .map((lang) => (lang === "*" ? "all" : lang))
      .join(",");
    const languages = baseLangs.includes("all")
      ? `${baseLangs},-live_chat,-description`
      : baseLangs;

    const args = [
      // Rate limiting to avoid detection
      "--sleep-requests", "1",
      "--sleep-subtitles", "3",

      // User-Agent spoofing
      "--user-agent",
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",

      // HTTP headers for browser-like behavior
      "--add-header", "Accept:text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      "--add-header", "Accept-Language:en-US,en;q=0.5",
      "--add-header", "Accept-Encoding:gzip, deflate",
      "--add-header", "DNT:1",
      "--add-header", "Connection:keep-alive",
      "--add-header", "Upgrade-Insecure-Requests:1",

      // Subtitle options
      "--write-auto-subs",
      "--write-subs",
      "--sub-langs", languages,
      "--skip-download",
      "--sub-format", "vtt",
      "--output", outputTemplate,
      "--no-warnings",
      "--no-progress",
    ];

    // Add extractor-args based on PO token availability
    if (tokenResult?.success && tokenResult.extractorArgs) {
      // Use PO token with mweb client
      args.push("--extractor-args", tokenResult.extractorArgs);
      logger.debug(
        {
          strategy: this.config.name,
          client: tokenResult.metadata?.client,
          cached: tokenResult.metadata?.cached,
        },
        "Using PO token for extraction"
      );
    } else {
      // Fallback: web_creator works for subtitles without PO tokens
      args.push("--extractor-args", "youtube:player_client=web_creator");
      logger.debug(
        { strategy: this.config.name, reason: tokenResult?.error },
        "Using fallback player client (no PO token)"
      );
    }

    // Verbose mode for debugging
    if (process.env.YTDLP_VERBOSE === "true") {
      args.unshift("--verbose");
    }

    // IPv6 source address
    if (sourceIp) {
      args.push("--source-address", sourceIp);
      logger.debug(
        { strategy: this.config.name, sourceIp: this.maskIPv6(sourceIp) },
        "Using rotated IPv6 address"
      );
    }

    // Cookies
    if (cookiesPath) {
      args.push("--cookies", cookiesPath);
    }

    args.push(videoUrl);

    return args;
  }

  private maskIPv6(address: string): string {
    const parts = address.split(":");
    if (parts.length >= 4) {
      return `${parts[0]}:${parts[1]}:...:${parts.at(-1)}`;
    }
    return address;
  }

  /**
   * Get PO Token service status (for health checks)
   */
  getPOTokenStatus() {
    return this.poTokenService?.getStatus() ?? null;
  }

  private async runCommand(
    command: string[],
    timeoutMs: number
  ): Promise<{ exitCode: number; stdout: string; stderr: string }> {
    return new Promise((resolve) => {
      const [cmd, ...args] = command;
      const proc = spawn(cmd, args, {
        stdio: ["ignore", "pipe", "pipe"],
        timeout: timeoutMs,
      });

      let stdout = "";
      let stderr = "";

      proc.stdout.on("data", (data) => {
        stdout += data.toString();
      });

      proc.stderr.on("data", (data) => {
        stderr += data.toString();
      });

      const timer = setTimeout(() => {
        proc.kill("SIGTERM");
        resolve({
          exitCode: -1,
          stdout,
          stderr: stderr + "\nProcess timed out",
        });
      }, timeoutMs);

      proc.on("close", (code) => {
        clearTimeout(timer);
        resolve({
          exitCode: code ?? -1,
          stdout,
          stderr,
        });
      });

      proc.on("error", (error) => {
        clearTimeout(timer);
        resolve({
          exitCode: -1,
          stdout,
          stderr: error.message,
        });
      });
    });
  }

  private async findAndParseVtt(
    extractionDir: string,
    includeTimestamps: boolean
  ): Promise<string | null> {
    try {
      const files = await readdir(extractionDir);
      const vttFiles = files.filter((f) => f.endsWith(".vtt"));

      if (vttFiles.length === 0) {
        return null;
      }

      // Prefer English subtitles
      const sortedVttFiles = vttFiles.sort((a, b) => {
        const aIsEnglish = a.includes(".en") ? 0 : 1;
        const bIsEnglish = b.includes(".en") ? 0 : 1;
        return aIsEnglish - bIsEnglish;
      });

      const vttPath = path.join(extractionDir, sortedVttFiles[0]);
      const vttContent = await readFile(vttPath, "utf8");

      return parseVttToTranscript(vttContent, includeTimestamps);
    } catch (error) {
      logger.debug({ extractionDir, error }, "Error finding/parsing VTT files");
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

  private parseYtdlpError(
    stderr: string,
    tokenResult?: TokenResult
  ): { message: string; type: TranscriptErrorType } {
    const lower = stderr.toLowerCase();

    if (lower.includes("private video")) {
      return {
        message: "This video is private",
        type: TranscriptErrorType.VIDEO_PRIVATE,
      };
    }

    if (
      lower.includes("video unavailable") ||
      lower.includes("this video is not available")
    ) {
      return {
        message: "This video is unavailable",
        type: TranscriptErrorType.VIDEO_UNAVAILABLE,
      };
    }

    if (lower.includes("live event") || lower.includes("live stream")) {
      return {
        message: "Live videos are not supported",
        type: TranscriptErrorType.VIDEO_LIVE,
      };
    }

    if (lower.includes("age") || lower.includes("sign in")) {
      return {
        message: "This video requires age verification",
        type: TranscriptErrorType.AGE_RESTRICTED,
      };
    }

    if (lower.includes("no subtitles") || lower.includes("no captions")) {
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

    if (lower.includes("timed out")) {
      return {
        message: "Request timed out",
        type: TranscriptErrorType.TIMEOUT,
      };
    }

    // PO Token related errors
    if (
      lower.includes("po token") ||
      lower.includes("proof of origin") ||
      lower.includes("potoken")
    ) {
      const hadToken = tokenResult?.success ?? false;
      logger.warn(
        {
          strategy: this.config.name,
          hadPOToken: hadToken,
          poTokenDegraded: tokenResult?.degraded,
        },
        "PO Token error detected"
      );
      return {
        message: hadToken
          ? "PO Token rejected by YouTube"
          : "PO Token required but not available",
        type: TranscriptErrorType.PO_TOKEN_ERROR,
      };
    }

    // HTTP 403 errors often indicate PO token issues
    if (lower.includes("http error 403") || lower.includes("403 forbidden")) {
      const hadToken = tokenResult?.success ?? false;
      if (!hadToken) {
        logger.warn(
          { strategy: this.config.name },
          "HTTP 403 without PO Token - provider may be down"
        );
      }
      return {
        message: "Access forbidden (HTTP 403) - PO token may be required",
        type: TranscriptErrorType.PO_TOKEN_ERROR,
      };
    }

    // Extract first meaningful line of error
    const firstLine = stderr.split("\n").find((l) => l.trim()) || stderr;
    return {
      message: firstLine.slice(0, 200) || "Unknown error from yt-dlp",
      type: TranscriptErrorType.UNKNOWN,
    };
  }
}
