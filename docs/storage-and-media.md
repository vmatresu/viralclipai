# Storage & Media (Cloudflare R2)

This document explains how video artifacts are stored in Cloudflare R2.

## Overview

Viral Clip AI uses **Cloudflare R2** as the primary object store for:

- Processed clip files (`.mp4`).
- Clip thumbnails (`.jpg`).
- Highlight metadata (`highlights.json`).

R2 is S3-compatible and offers zero egress fees, which is ideal for a
video-heavy SaaS fronted by a CDN.

## Configuration

Backend configuration is handled in `app/config.py` and environment variables:

- `R2_ACCOUNT_ID`
- `R2_BUCKET_NAME`
- `R2_ACCESS_KEY_ID`
- `R2_SECRET_ACCESS_KEY`
- `R2_ENDPOINT_URL`

See `docs/configuration.md` for details.

## Bucket Layout

R2 object keys are namespaced per user and per job (run id):

- `users/{uid}/{run_id}/highlights.json`
- `users/{uid}/{run_id}/clips/clip_<style>_<n>.mp4`
- `users/{uid}/{run_id}/clips/clip_<style>_<n>.jpg`

This layout allows for simple per-user purging and minimizes the risk of
collisions.

## API Usage

`app/core/storage.py` encapsulates all interaction with R2:

- Creates a boto3 S3 client using the configured endpoint and credentials.
- Uploads files with content-type metadata.
- Lists clip objects and corresponding thumbnails.
- Generates presigned URLs for secure, time-limited access.

### Highlights

- `load_highlights(uid, video_id)` loads and parses `highlights.json` from R2.
- `highlights.json` includes `video_url`, optional `custom_prompt`, and a list
  of highlight entries used to drive the clip grid in the frontend.

### Clips & Thumbnails

- `list_clips_with_metadata(...)` paginates through R2 objects under
  `users/{uid}/{video_id}/clips/` and:
  - filters for `.mp4` files
  - finds matching `.jpg` thumbnails
  - generates presigned URLs for both
  - applies titles/descriptions from `highlights_map`

## CDN & Access

In most deployments, R2 sits behind Cloudflare's global network:

- Backend generates presigned URLs for each clip and thumbnail.
- Clients access media via these short-lived URLs, which are cached at the
  edge by Cloudflare.

For production setups, ensure:

- R2 bucket policies are locked down to prevent public listing.
- Only presigned URLs or trusted services can access objects.

For prompt behavior, see `docs/prompts.md`. For overall architecture, see
`docs/architecture.md`.
