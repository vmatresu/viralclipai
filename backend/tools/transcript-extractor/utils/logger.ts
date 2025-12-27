/**
 * Structured Logger
 *
 * Pino-based logger with JSON output for structured logging.
 * Outputs to stderr to keep stdout clean for JSON results.
 */

import pino from "pino";

const LOG_LEVEL = process.env.LOG_LEVEL || "info";

export const logger = pino({
  level: LOG_LEVEL,
  transport:
    process.env.NODE_ENV === "development"
      ? {
          target: "pino-pretty",
          options: {
            colorize: true,
            translateTime: "SYS:standard",
            ignore: "pid,hostname",
          },
        }
      : undefined,
  // Output to stderr so stdout remains clean for JSON output
  // This is critical for CLI usage where Rust parses stdout
});

export default logger;
