# Architecture (Rust Backend)

This document describes the high-level architecture of the current Viral Clip AI stack, centered around the Rust backend.

## Overview

Viral Clip AI is a multi-tenant SaaS that turns long-form YouTube content into short, viral clips.

- **Backend**: Rust (Axum, Tokio), organized as a Cargo workspace with multiple crates
- **Worker**: Rust background worker for video processing and Gemini analysis
- **Frontend**: Next.js (App Router) + React + TypeScript + TailwindCSS
- **AI**: Google Gemini for highlight detection
- **Storage**: Cloudflare R2 (S3-compatible) for media artifacts
- **Auth & Data**: Firebase Auth + Firestore
- **Queue**: Redis + Apalis for job scheduling

For a deep-dive into the clip pipeline, see `docs/video-processing-pipeline.md`.

## Backend Workspace Layout

The Rust backend lives under `backend/` as a Cargo workspace:

```text
backend/
  Cargo.toml              # Workspace + shared deps
  crates/
    vclip-api             # HTTP & WebSocket API (Axum)
    vclip-worker          # Background job worker
    vclip-media           # FFmpeg-based media operations
    vclip-storage         # Cloudflare R2 client + presigned URLs
    vclip-firestore       # Firestore repository layer
    vclip-ml-client       # Gemini client and AI helpers
    vclip-queue           # Redis/Apalis queue integration
    vclip-models          # Shared types and enums
```

### `vclip-api` (HTTP & WebSocket API)

Responsibilities:

- HTTP API endpoints (Axum routers)
- WebSocket endpoint for processing jobs
- Request validation and authentication (Firebase ID tokens)
- Rate limiting, CORS, security headers
- Exposes video history and clip metadata

Key concepts:

- `ApiConfig` (`vclip-api/src/config.rs`) loads API-level settings from env vars
- Uses `tower-http` for CORS, tracing, compression, and limits
- Emits structured logs and metrics for observability

### `vclip-worker` (Background Processing)

Responsibilities:

- Consumes processing jobs from Redis (Apalis-based queue)
- Orchestrates the full video pipeline:
  - Download source video
  - Run Gemini analysis to produce `highlights.json`
  - Generate clip tasks
  - Invoke `vclip-media` to render clips & thumbnails
  - Upload artifacts to R2 via `vclip-storage`
  - Persist metadata to Firestore via `vclip-firestore`
- Tracks progress and updates Firestore status

See `docs/video-processing-pipeline.md` for detailed phase breakdown.

### `vclip-media` (Media/FFmpeg)

Responsibilities:

- Building FFmpeg commands in a safe, typed way
- Implementing different styles (split, left/right focus, original, intelligent_split)
- Handling basic clip creation and complex multi-step pipelines
- Generating thumbnails

Related docs:

- `docs/STYLES_AND_CROP_MODES.md` – overview of styles and crop modes
- `docs/rust-style-processing-architecture.md` – mapping from original Python design to Rust implementation

### `vclip-storage` (R2 Storage)

Responsibilities:

- Managing Cloudflare R2 client configuration
- Uploading highlights, clips, and thumbnails
- Generating presigned URLs for secure, time-limited access
- Enforcing consistent key layout per user and video

See `docs/storage-and-media.md` and `r2-setup.md` for configuration details.

### `vclip-firestore` (Metadata & Persistence)

Responsibilities:

- Type-safe access to Firestore for:
  - User documents (`users/{uid}`)
  - Video metadata (`users/{uid}/videos/{video_id}`)
  - Clip metadata (`users/{uid}/videos/{video_id}/clips/{clip_id}`)
- Encapsulating queries, indexes, and common patterns
- Handling status transitions (processing → completed/failed)

The Firestore-based design replaces slow R2 listing with a dedicated metadata layer. The data model is documented in `docs/video-processing-pipeline.md` and referenced by the API.

### `vclip-ml-client` (Gemini & AI)

Responsibilities:

- Integrating with Google Gemini for highlight extraction
- Handling transcript processing (e.g. VTT → structured text)
- Implementing model selection and fallback strategies
- Applying base prompts and user-specific custom prompts

Prompt behavior and configuration are described in `docs/prompts.md`.

### `vclip-queue` (Redis + Apalis)

Responsibilities:

- Defining job payload types used by the worker
- Integrating with Redis for durable job queues
- Exposing a simple API to enqueue processing jobs from `vclip-api`

This keeps API request handling fast and offloads heavy work to the worker.

### `vclip-models` (Shared Types)

Responsibilities:

- Shared enums and structs used across crates (e.g. `Style`, `CropMode`)
- Serializable/validatable types for public API payloads and internal messages

## Frontend (Next.js)

The frontend lives under `web/` and is responsible for the user experience:

- **Framework**: Next.js App Router + React + TypeScript
- **Styling**: TailwindCSS
- **Auth**: Firebase Web SDK
- **Analytics**: Firebase Analytics (see `docs/analytics.md`)

Key areas:

- `web/app` – routes (landing, processing UI, history, admin, etc.)
- `web/components/ProcessingClient` – main clip-generation UI + WebSocket client
- `web/components/ClipGrid` – clip display and TikTok publish flows
- `web/lib/auth` – Firebase auth and ID token handling
- `web/lib/apiClient` – thin client for the Rust API

## Data Flow (High Level)

1. **User authenticates** via Firebase in the frontend
2. **User submits a YouTube URL** and style(s) via WebSocket to the API
3. **API enqueues a job** into Redis (`vclip-queue`)
4. **Worker consumes the job** (`vclip-worker`):
   - Downloads video
   - Runs Gemini via `vclip-ml-client`
   - Writes `highlights.json` to R2 via `vclip-storage`
   - Generates clips via `vclip-media`
   - Persists metadata via `vclip-firestore`
5. **Frontend receives progress events** over WebSocket
6. **Once done**, the frontend fetches video + clip metadata via HTTP and renders the grid

For security and performance aspects of this flow, see:

- `docs/video-processing-pipeline.md`
- `docs/logging-and-observability.md`
- `docs/storage-and-media.md`
