# Architecture

This document describes the high-level architecture of the Viral Clip AI platform.

## Overview

Viral Clip AI is a multi-tenant SaaS that turns long-form YouTube content into short, viral clips. It consists of:

- **FastAPI backend** for API, video processing orchestration, and integrations.
- **Next.js frontend** (App Router) for the user-facing web app.
- **Google Gemini** for AI highlight detection.
- **Cloudflare R2** (S3-compatible) for clip, thumbnail, and metadata storage.
- **Firebase Auth + Firestore** for authentication, user data, and job metadata.
- **TikTok integration** for publishing clips.

## Backend

- **Framework**: FastAPI (async)
- **Entrypoint**: `app.main:app`
- **Key modules**:
  - `app/core/workflow.py` – orchestrates the end-to-end video processing workflow.
  - `app/core/gemini.py` – calls Google Gemini with prompts and video URL.
  - `app/core/clipper.py` – creates actual clips using ffmpeg, based on highlights.
  - `app/core/saas.py` – plans, quotas, user/video metadata in Firestore.
  - `app/core/storage.py` – Cloudflare R2 client, upload and presigned URLs.
  - `app/routers/web.py` – WebSocket `/ws/process` and REST endpoints.
  - `app/config.py` – configuration and logging setup.

### Workflow

1. **Client** sends a WebSocket message to `/ws/process` with:
   - Firebase ID token
   - YouTube URL
   - Output style(s)
   - Optional custom prompt
2. Backend verifies token, creates a per-run workdir under `videos/{run_id}`.
3. Backend downloads the YouTube video and prepares audio/video for analysis.
4. Gemini is called with:
   - A base prompt (global or per-user; see `docs/prompts.md`).
   - The video URL/reference.
5. Gemini returns structured "highlights" metadata.
6. `clipper` turns each highlight into one or more `.mp4` clips and thumbnails.
7. Clips, thumbnails, and `highlights.json` are uploaded to Cloudflare R2.
8. URLs and metadata are exposed via REST to the frontend.

## Frontend

- **Framework**: Next.js 14 (App Router) + React + TypeScript
- **Styling**: TailwindCSS
- **Auth**: Firebase Web SDK
- **Key areas**:
  - `web/app` – route structure (landing, history, docs, admin pages, etc.).
  - `web/components/ProcessingClient.tsx` – main clip-generation UI with WebSocket.
  - `web/components/ClipGrid.tsx` – display of generated clips.
  - `web/lib/auth.ts` – Firebase auth and ID token retrieval.
  - `web/lib/apiClient.ts` – thin wrapper around the backend API.
  - `web/lib/logger.ts` – frontend logging abstraction.

### Data Flow

- Browser authenticates via Firebase and obtains an ID token.
- For processing:
  - Opens a WebSocket to `/ws/process` with `{ url, style, token, prompt? }`.
  - Receives progress/log messages and the final `videoId`.
- For results:
  - Calls `GET /api/videos/{videoId}` to fetch clip metadata and the prompt used.
- For history:
  - Calls `GET /api/user/videos` for a list of past jobs.

## Data Model

### Firestore

Collections/documents (simplified):

- `users/{uid}`
  - `email`
  - `plan` (e.g. `free`, `pro`)
  - `role` (optional, e.g. `superadmin`)
  - `settings` (misc per-user settings)
- `users/{uid}/videos/{run_id}`
  - `video_id` (alias of `run_id`)
  - `video_url`
  - `video_title`
  - `clips_count`
  - `created_at`
  - `custom_prompt` (if set for that job)
- `admin/config`
  - `base_prompt` (global default prompt)
  - `updated_at`
  - `updated_by`

### Cloudflare R2

Bucket structure (per user and job):

- `users/{uid}/{run_id}/highlights.json`
- `users/{uid}/{run_id}/clips/clip_<style>_<n>.mp4`
- `users/{uid}/{run_id}/clips/clip_<style>_<n>.jpg` (thumbnail)

`highlights.json` contains:

- `video_url`
- `custom_prompt` (if set)
- `highlights`: list of highlight objects (timestamps, titles, descriptions).

## Security & Multi-Tenancy

- **Auth**: All authenticated routes require a valid Firebase ID token.
- **Ownership**: For any `video_id`, backend verifies the calling `uid` owns it.
- **Plans & quotas**: Plan configuration in `saas.PLAN` controls monthly clip limits.
- **Storage isolation**: R2 keys are always namespaced by `uid` and `run_id`.

For environment and deployment details, see:

- `docs/configuration.md`
- `docs/deployment.md`
