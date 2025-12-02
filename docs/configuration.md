# Configuration & Environment

This document describes how to configure Viral Clip AI via environment variables and supporting files.

## Overview

Configuration is driven primarily by environment variables, loaded by `app/config.py` on the backend and `.env.*` files on the frontend.

For full examples, see:

- `.env.api.example` – backend production example.
- `.env.api.dev.example` – backend development example.
- `web/.env.local.example` – Next.js local development.
- `web/.env.production.example` – Next.js production.

## Backend (FastAPI)

### Core

- `LOG_LEVEL` – logging level (e.g. `DEBUG`, `INFO`, `WARNING`).
- `LOG_FILE_PATH` – optional path for the rotating log file. Defaults to `logs/app.log`.

### Gemini

- `GEMINI_API_KEY` – Google Gemini API key used by `app/core/gemini.py`.

### Firebase Admin / Firestore

- `FIREBASE_PROJECT_ID` – Firebase project ID.
- `FIREBASE_CREDENTIALS_PATH` – absolute path inside the container/VM to a
  Firebase service account JSON with Firestore access.

### Cloudflare R2 (S3-compatible storage)

- `R2_ACCOUNT_ID` – Cloudflare account ID.
- `R2_BUCKET_NAME` – R2 bucket name used for all media artifacts.
- `R2_ACCESS_KEY_ID` – R2 API access key.
- `R2_SECRET_ACCESS_KEY` – R2 API secret key.
- `R2_ENDPOINT_URL` – optional custom endpoint. If empty, defaults to
  `https://<R2_ACCOUNT_ID>.r2.cloudflarestorage.com`.

### TikTok API

- `TIKTOK_API_BASE_URL` – base URL for the TikTok upload endpoint (or proxy)
  used in `app/core/tiktok_client.py`.

### Security / Domains

- `ALLOWED_HOSTS` – comma-separated list of allowed hosts for FastAPI's
  `TrustedHostMiddleware`.
- `CORS_ORIGINS` – comma-separated list of allowed browser origins.

## Frontend (Next.js)

The Next.js app uses `.env`-style files in `web/`.

### Common variables

- `NEXT_PUBLIC_API_BASE_URL` – base URL of the backend API exposed to browsers.
- `NEXT_PUBLIC_FIREBASE_API_KEY` – Firebase Web API key.
- `NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN` – Firebase auth domain.
- `NEXT_PUBLIC_FIREBASE_PROJECT_ID` – Firebase project ID.
- `NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET` – Firebase storage bucket.
- `NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID` – Firebase messaging sender ID.
- `NEXT_PUBLIC_FIREBASE_APP_ID` – Firebase app ID.
- `NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID` – (Optional) Firebase Analytics measurement ID for Google Analytics 4 integration.

### Local Development

`web/.env.local` example:

```bash
NEXT_PUBLIC_API_BASE_URL=http://localhost:8000
NEXT_PUBLIC_FIREBASE_API_KEY=...
...
```

### Production

`web/.env.production` (or Vercel project env vars) should mirror the same keys
but point to your production backend and Firebase project.

## Paths

Key paths (relative to project root):

- `videos/` – local scratch space used during processing.
- `logs/app.log` – default backend log file (rotating).
- `prompt.txt` – local prompt fallback when no global admin prompt is set.

For details on prompt behavior, see `docs/prompts.md`. For storage and R2
layout, see `docs/storage-and-media.md`.
