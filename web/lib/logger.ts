import pino, { type Logger } from "pino";

type LogLevel = "debug" | "info" | "warn" | "error";

const logLevel: LogLevel =
  (process.env.NEXT_PUBLIC_LOG_LEVEL as LogLevel) ||
  (process.env.NODE_ENV === "production" ? "info" : "debug");

const pinoConfig = {
  level: logLevel,
  browser: {
    asObject: true, // Log as objects in browser console for easier inspection
    serialize: true,
  },
  // In development (server-side), use pino-pretty for readable logs
  ...(typeof window === "undefined" && process.env.NODE_ENV !== "production"
    ? {}
    : {}),
  // In production, clean JSON structure is preferred
  timestamp: pino.stdTimeFunctions.isoTime,
};

export const logger: Logger = pino(pinoConfig);

// Compatibility wrapper for existing code using frontendLogger
export const frontendLogger = {
  info: (message: string, ...meta: unknown[]) => logger.info({ meta }, message),
  warn: (message: string, ...meta: unknown[]) => logger.warn({ meta }, message),
  error: (message: string, ...meta: unknown[]) => logger.error({ meta }, message),
  debug: (message: string, ...meta: unknown[]) => logger.debug({ meta }, message),
};