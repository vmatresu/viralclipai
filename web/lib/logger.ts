export type LogLevel = "info" | "warn" | "error";

function log(level: LogLevel, message: string, ...meta: unknown[]): void {
  const isBrowser = typeof window !== "undefined";

  // Server-side (Next.js Node runtime): send to console for now so logs end up
  // in the hosting provider's log stream.
  if (!isBrowser) {
    if (level === "error") {
      // eslint-disable-next-line no-console
      console.error(message, ...meta);
    } else if (level === "warn") {
      // eslint-disable-next-line no-console
      console.warn(message, ...meta);
    } else {
      // eslint-disable-next-line no-console
      console.log(message, ...meta);
    }
    return;
  }

  // Browser runtime: keep console noise low in production, but always show
  // errors. In dev, log everything to help debugging.
  const isProd = process.env.NODE_ENV === "production";

  if (level === "error") {
    // eslint-disable-next-line no-console
    console.error(message, ...meta);
    return;
  }

  if (!isProd) {
    if (level === "warn") {
      // eslint-disable-next-line no-console
      console.warn(message, ...meta);
    } else {
      // eslint-disable-next-line no-console
      console.log(message, ...meta);
    }
  }
}

export const frontendLogger = {
  info: (message: string, ...meta: unknown[]) => log("info", message, ...meta),
  warn: (message: string, ...meta: unknown[]) => log("warn", message, ...meta),
  error: (message: string, ...meta: unknown[]) => log("error", message, ...meta),
};
