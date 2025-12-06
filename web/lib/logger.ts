type LogLevel = "debug" | "info" | "warn" | "error";

const logLevel: LogLevel =
  (process.env.NEXT_PUBLIC_LOG_LEVEL as LogLevel) ||
  (process.env.NODE_ENV === "production" ? "info" : "debug");

// Simple console-based logger
const createLogger = () => ({
  info: (message: string, ...meta: unknown[]) => {
    if (logLevel === "debug" || logLevel === "info") {
      console.info(`[INFO] ${message}`, ...meta);
    }
  },
  warn: (message: string, ...meta: unknown[]) => {
    if (logLevel === "debug" || logLevel === "info" || logLevel === "warn") {
      console.warn(`[WARN] ${message}`, ...meta);
    }
  },
  error: (message: string, ...meta: unknown[]) => {
    console.error(`[ERROR] ${message}`, ...meta);
  },
  debug: (message: string, ...meta: unknown[]) => {
    if (logLevel === "debug") {
      console.debug(`[DEBUG] ${message}`, ...meta);
    }
  },
});

export const logger = createLogger();

// Compatibility wrapper for existing code using frontendLogger
export const frontendLogger = logger;
