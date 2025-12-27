/**
 * Configuration Loader
 *
 * Loads configuration from environment variables with sensible defaults.
 * Follows the 12-factor app methodology.
 */

export interface AppConfig {
  /** Node environment */
  nodeEnv: string;
  /** YouTube Data API key */
  youtubeApiKey?: string;
  /** Apify API token */
  apifyToken?: string;
  /** Path to YouTube cookies file for yt-dlp */
  ytdlpCookiesPath?: string;
  /** yt-dlp binary path */
  ytdlpPath: string;
  /** Working directory for temp files */
  workDir: string;
  /** Log level */
  logLevel: string;
  /** Whether PO Token provider is enabled for yt-dlp */
  ytdlpPOTokenEnabled: boolean;
}

/**
 * Parse boolean from environment variable
 */
function parseBoolean(value: string | undefined, defaultValue: boolean): boolean {
  if (value === undefined) return defaultValue;
  return value.toLowerCase() === "true";
}

/**
 * Load configuration from environment
 */
function loadConfig(): AppConfig {
  return {
    nodeEnv: process.env.NODE_ENV?.trim() || "development",
    youtubeApiKey: process.env.YOUTUBE_API_KEY?.trim() || undefined,
    apifyToken: process.env.APIFY_TOKEN?.trim() || undefined,
    ytdlpCookiesPath: process.env.YTDLP_COOKIES_PATH?.trim() || undefined,
    ytdlpPath: process.env.YTDLP_PATH?.trim() || "yt-dlp",
    workDir:
      process.env.TRANSCRIPT_WORK_DIR?.trim() || "/tmp/transcript-extraction",
    logLevel: process.env.LOG_LEVEL?.trim() || "info",
    ytdlpPOTokenEnabled: parseBoolean(process.env.POT_ENABLED, true),
  };
}

export const Config = loadConfig();
export default Config;
