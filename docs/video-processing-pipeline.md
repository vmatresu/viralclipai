# Video Processing Pipeline (Current)

This supersedes older drafts and reflects the modular pipeline after the `processor.rs` + `clip_pipeline` split.

## Phase Flow

1. **Ingest**

   - Create per-job work directory.
   - Create/refresh Firestore video record (`status: processing`).
   - Emit initial progress events.

2. **AI Analysis (highlights-first)**

   - Fetch transcript (youtubei.js tool with yt-dlp fallback).
   - Cache transcript to R2 for reuse.
   - Run Gemini transcript + highlight extraction.
   - Require non-empty highlights; otherwise fail fast and mark video failed.
   - Upload `highlights.json` to R2 (`{user}/{video}/highlights.json`) — single source of truth for clips.

3. **Clip Generation (on-demand)**

   - `clip_pipeline/tasks.rs`: expand `(highlight × style)` into `ClipTask`s with crop mode, target aspect, padding.
   - `clip_pipeline/scene.rs`: group tasks by scene; emit scene events; fan out styles in parallel using the shared FFmpeg semaphore.
   - `clip_pipeline/clip.rs`: per-clip work
     - Download source video only when rendering is requested (reprocess/render jobs).
     - Build `ProcessingRequest` with style-specific encoding (intelligent vs split vs static).
     - Delegate to `vclip-media` style processors (`run_basic_style` for static, tier-aware engines for intelligent).
     - **Watermark overlay** (free users only): injected into the FFmpeg filter graph during rendering (single encode, no post-pass). `clip_pipeline` sets `ProcessingRequest.watermark` via `user_plan`.
     - Upload clip + thumbnail to R2; persist `ClipMetadata` to Firestore; emit progress and legacy `clip_uploaded`.

4. **Finalize**
   - Mark video completed in Firestore with clip count.
   - Emit final progress/done events.
   - Best-effort cleanup of work dir.

## Contracts & Artefacts

- **R2 layout**: `{user_id}/{video_id}/highlights.json` + `clips/*.mp4` + `clips/*.jpg` (same stem).
- **Transcript cache**: `{user_id}/transcripts/{cache_id}.txt.gz` (manual cleanup).
- **Firestore**: authoritative metadata for videos/clips; API never lists R2 for truth.
- **Progress**: scene/clip started → rendering → uploaded → complete; warnings for non-critical thumbnail upload failures.

## Roles by Crate

- `vclip-worker`: orchestration (`processor.rs`), `clip_pipeline` (tasks/scene/clip), progress channel, `user_plan` (plan resolution), `watermark_check`.
- `vclip-media`: style processors, FFmpeg runners, thumbnails, intelligent crop/split engines, `watermark` overlay module.
- `vclip-storage`: R2 uploads + presigned URLs.
- `vclip-firestore`: repositories for videos/clips + status transitions.
- `vclip-ml-client`: Gemini calls + prompt handling.
- `vclip-queue`: Redis/Apalis job types & enqueue/consume.
- `vclip-api`: Axum HTTP/WebSocket API; blocks clip access until `highlights.json` exists; enforces auth/ownership.

## Safety & Quality Gates

- **Highlights-first rule**: clips are only served if `highlights.json` is present.
- **Resource controls**: FFmpeg semaphore, sanitized commands, timeouts in media layer.
- **Error handling**: fail fast on ingest/AI; continue-on-error per clip.
- **Security**: Firebase auth at API, presigned R2 URLs, validated IDs/paths, CORS/rate limits.
- **Plans & quotas**: before a job starts, WebSocket and REST handlers check monthly clip quotas and storage limits; jobs that would exceed limits are rejected early. After each clip, the worker updates video and user storage counters using optimistic concurrency. See `plans-and-quotas.md`.
- **Observability**: structured tracing, per-phase progress, metrics on durations/errors.

## How to extend

- Add a style: implement `StyleProcessor`, register in `StyleProcessorFactory`; map encoding if needed.
- Add an analysis signal: extend `highlights.json` and pass through `ClipTask` generation.
- Tune performance: adjust semaphore sizes (`WORKER_MAX_FFMPEG`), encoding presets, or scene parallelism.
