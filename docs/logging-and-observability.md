# Logging & Observability

This document describes how logging works in Viral Clip AI on both backend and frontend.

## Backend Logging

Backend logging is configured centrally in `app/config.py` using
`logging.config.dictConfig` with a rotating file handler.

### Configuration

- **Log level**: controlled by `LOG_LEVEL` env var (default `DEBUG`).
- **Log file path**: controlled by `LOG_FILE_PATH` env var (default
  `logs/app.log`).
- **Handlers**:
  - Console (stdout) – for container logs and local dev.
  - Rotating file – up to 10 MB per file, 5 backups.

You should ensure the `logs/` directory is writable and include it in your
log shipping / aggregation solution (e.g. CloudWatch, Datadog, Loki).

### Usage

Modules use `logging.getLogger(__name__)` or the `viralclipai` logger from
`app/config.py`. Avoid using `print()` and prefer structured log messages.

## Frontend Logging

The frontend uses a small abstraction in `web/lib/logger.ts` instead of direct
`console.log` calls.

- Methods: `info`, `warn`, `error`.
- In production, logging can be minimized or forwarded to an external
  monitoring service (e.g. Sentry, LogRocket) by adapting this module.

Key places where it is used:

- `web/components/ProcessingClient.tsx` for WebSocket and processing errors.
- `web/components/ClipGrid.tsx` for TikTok publish flows.
- `web/lib/auth.ts` for Firebase configuration issues.

## Metrics & Monitoring (Suggested)

While not enforced by the codebase, recommended practices include:

- Add application-level metrics (requests, processing duration, ffmpeg errors)
  via your preferred telemetry stack.
- Monitor R2 usage and error rates via Cloudflare analytics.
- Monitor Firestore read/write quotas and latency.

## Error Handling

- Backend surfaces safe error messages over WebSocket and REST while logging
  full traces to the log file.
- Frontend displays user-friendly messages while logging detailed information
  via the `logger` abstraction.

For deployment considerations, see `docs/deployment.md`.
