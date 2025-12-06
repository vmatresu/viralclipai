# Configuration & Environment (Rust Backend)

This document describes how to configure Viral Clip AI using environment variables and `.env` files for the current Rust + Next.js stack.

Most configuration is driven by environment variables, loaded by the Rust backend (via `config`/`dotenvy`) and the Next.js frontend (`.env.*` files).

## Env Files Overview

At the repo root / in the `web/` folder you will typically use:

- `.env.api` / `.env.api.dev` – backend configuration for production/dev
- `.env` / `.env.dev` – convenience files loaded by Docker Compose
- `web/.env.local` – frontend local development
- `web/.env.production` – frontend production (or env settings in Vercel, etc.)

The backend containers read env vars directly; you do **not** need to mount `.env` inside Docker images if you prefer to inject env at deploy time.

## Backend (Rust)

### Core API Settings

These map directly to `ApiConfig` in `backend/crates/vclip-api/src/config.rs`:

- `API_HOST` – bind host (default `0.0.0.0`)
- `API_PORT` – bind port (default `8000`)
- `CORS_ORIGINS` – comma-separated list of allowed browser origins
- `RATE_LIMIT_RPS` – requests per second per client (default `10`)
- `RATE_LIMIT_BURST` – allowed burst above steady rate (default `20`)
- `REQUEST_TIMEOUT` – per-request timeout in seconds (default `30`)
- `MAX_BODY_SIZE` – max request body size in bytes (default `10 * 1024 * 1024`)
- `ENVIRONMENT` – `development` or `production` (controls security hardening, error messages, etc.)

### Gemini (Google AI)

- `GEMINI_API_KEY` – API key for Google Gemini

The worker uses this to call multiple Gemini models with a robust fallback strategy. See `docs/video-processing-pipeline.md` and `docs/prompts.md` for behavior.

### Firebase Admin / Firestore

- `FIREBASE_PROJECT_ID` – Firebase project ID
- `FIREBASE_CREDENTIALS_PATH` – absolute path (inside container/VM) to a
  service account JSON with Firestore access

The Rust `vclip-firestore` crate uses these to authenticate via `gcp_auth` and talk to Firestore.

### Cloudflare R2 (S3-Compatible Storage)

- `R2_ACCOUNT_ID` – Cloudflare account ID
- `R2_BUCKET_NAME` – name of the R2 bucket for all media artifacts
- `R2_ACCESS_KEY_ID` – R2 API access key
- `R2_SECRET_ACCESS_KEY` – R2 API secret key
- `R2_ENDPOINT_URL` – jurisdiction-specific S3 endpoint
- `R2_REGION` – usually `auto`
- `R2_PUBLIC_URL` – optional CDN domain for serving public media (e.g. `https://cdn.yourdomain.com`)

See `r2-setup.md` and `docs/storage-and-media.md` for details.

### TikTok Integration

- `TIKTOK_API_BASE_URL` – base URL for the TikTok upload endpoint or proxy used by the backend when publishing clips.

### Security / Domains

- `ALLOWED_HOSTS` – comma-separated list of allowed hostnames at the API level
- `CORS_ORIGINS` – comma-separated allowed origins for browsers
- `ENVIRONMENT` – set to `production` to enable security hardening (sanitized errors, stricter defaults)

These are used by the API to configure CORS, trusted hosts, and other middleware.

### Paths & Operational Settings

- `WORK_DIR` (if present) – base directory for per-job workspaces (default usually `./videos/`)
- `LOG_LEVEL` – log level filter (e.g. `info`, `debug`, `warn`)
- `OTEL_EXPORTER_OTLP_ENDPOINT` – if set, enables OpenTelemetry OTLP export

The worker creates a per-video directory under the configured work directory, then cleans it up after processing (see `docs/video-processing-pipeline.md`).

## Frontend (Next.js)

The frontend uses `.env`-style files under `web/` and `NEXT_PUBLIC_*` vars so they can be safely exposed to the browser.

### API & Firebase

- `NEXT_PUBLIC_API_BASE_URL` – base URL of the Rust API
- `NEXT_PUBLIC_FIREBASE_API_KEY` – Firebase Web API key
- `NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN` – Firebase auth domain
- `NEXT_PUBLIC_FIREBASE_PROJECT_ID` – Firebase project ID
- `NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET` – Firebase storage bucket
- `NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID` – Firebase messaging sender ID
- `NEXT_PUBLIC_FIREBASE_APP_ID` – Firebase app ID
- `NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID` – (optional) Analytics measurement ID

These are read by the Firebase client and analytics modules in `web/lib`.

### Local Development Example

`web/.env.local`:

```bash
NEXT_PUBLIC_API_BASE_URL=http://localhost:8000
NEXT_PUBLIC_FIREBASE_API_KEY=...
NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=...
NEXT_PUBLIC_FIREBASE_PROJECT_ID=...
...
```

## Recommended Setup

- **Per-environment separation**: keep distinct env files for `dev`, `staging`, and `prod`
- **Secrets management**: in production, inject env vars via your orchestrator (GitHub Actions, Docker, cloud provider) rather than committing `.env` files
- **Consistency**: ensure `FIREBASE_PROJECT_ID` and `NEXT_PUBLIC_FIREBASE_PROJECT_ID` refer to the same project per environment
- **Validation**: on startup, the API and worker should log missing/invalid critical env vars (e.g. `GEMINI_API_KEY`) as errors

For operational deployment details, see `docs/deployment.md`.
