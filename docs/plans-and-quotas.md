# Plans, Quotas, and Storage Limits

## Overview

ViralClip AI is sold as a SaaS product with usage-based **plans**. Each plan specifies limits on:

- **Monthly clips per user**
- **Total storage per user (all clips across all videos)**
- **Highlight and style limits per video**
- **Access to higher intelligent tiers / reprocessing**

This document explains how plans and quotas are modeled, where they are enforced, and how they surface in the UI. It is intended for backend, worker, and frontend contributors.

See also:

- `storage-and-media.md` – physical storage layout (R2/Cloud Storage) and file naming
- `INTELLIGENT_TIERS_DESIGN.md` – detection tiers and how higher tiers map to plans
- `configuration.md` – environment and deployment-time configuration

---

## Plan tiers and limits

### Plan tiers

The core plan tiers are defined in Rust in `backend/crates/vclip-models/src/plan.rs`:

- **Free**
  - `PlanTier::Free`
  - 200 credits / month
  - 1 GB storage (`FREE_STORAGE_LIMIT_BYTES`)
  - Limited highlights/styles per video
  - No reprocessing
  - **Watermark on exports** (branded overlay applied)
- **Pro**
  - `PlanTier::Pro`
  - 4,000 credits / month
  - 30 GB storage (`PRO_STORAGE_LIMIT_BYTES`)
  - More highlights/styles per video
  - Reprocessing enabled
  - Watermark-free exports
- **Studio**
  - `PlanTier::Studio`
  - 12,000 credits / month
  - 150 GB storage (`STUDIO_STORAGE_LIMIT_BYTES`)
  - Highest highlights/styles limits
  - Reprocessing enabled
  - Watermark-free exports

The exact numeric defaults live in `PlanLimits::for_tier` and are surfaced through the `PlanLimits` struct exported from `vclip-models`.

### Backend source of truth

The backend is the **source of truth** for plan tiers and storage limits:

- `PlanTier` – enum of `free`, `pro`, `studio` with parsing from strings.
- `PlanLimits` – per-plan configuration:
  - `plan_id: String`
  - `max_clips_per_month: u32`
  - `max_highlights_per_video: u32`
  - `max_styles_per_video: u32`
  - `can_reprocess: bool`
  - `storage_limit_bytes: u64`
- Plan defaults are computed in code based on `PlanTier`.

Plan documents in Firestore (e.g. `plans/{planId}`) may optionally override clip and storage limits via a `limits` map:

- `limits.max_clips_per_month: number`
- `limits.storage_limit_bytes: number`

If these fields are missing, the backend falls back to the in-code defaults based on `plan_id`.

### Frontend alignment

The frontend mirrors these limits for display only:

- `web/types/storage.ts` defines `PLAN_STORAGE_LIMITS` with the same byte limits:
  - Free – 1 GB
  - Pro – 30 GB
  - Studio – 150 GB
- `web/app/pricing/page.tsx` lists the same storage values as part of the marketing copy (`"1 GB storage"`, `"30 GB storage"`, `"150 GB storage"`).

The frontend must not enforce limits independently; it consumes the values returned by the backend and only **displays** usage / blocks actions based on server-validated state.

---

## Storage accounting model

### Counters and entities

Storage and quota accounting are driven by a set of counters stored in Firestore and derived from clip metadata:

- **Clip level** (per output clip)
  - `file_size_bytes` – stored in the clip document in the `clips` collection.
- **Video level** (`VideoMetadata` in `vclip-models::video`)
  - `clips_count: u32` – number of completed clips for the video.
  - `total_size_bytes: u64` – sum of `file_size_bytes` for all clips of the video.
- **User level** (`UserRecord` in `UserService`)
  - `total_storage_bytes: u64` – sum of `file_size_bytes` for all clips across all videos.
  - `total_clips_count: u32` – total number of clips across all videos.

The logical invariant we aim to maintain is:

- `user.total_storage_bytes` ≈ Σ(all clip.file_size_bytes for that user)
- `user.total_clips_count` ≈ number of clip documents for that user
- For each video:
  - `video.total_size_bytes` ≈ Σ(file_size_bytes for all clips for that video)
  - `video.clips_count` = number of completed clips for that video

These invariants are maintained incrementally and can be fully recomputed using migration / reconciliation paths when drift is detected.

### Storage usage abstraction

`vclip-models::StorageUsage` wraps the raw counters into a consistent API:

- `StorageUsage::new(total_bytes, total_clips, limit_bytes)`
- `percentage() -> f64` – usage percentage, capped at 100% when over limit
- `would_exceed(additional_bytes: u64) -> bool` – returns `true` if adding bytes would cross the limit
- `remaining_bytes() -> u64` – limit minus total, saturating at 0
- `format_total()` / `format_limit()` / `format_remaining()` – human-readable `"X.XX MB"` style strings

The API layer and UI use `StorageUsage` rather than duplicating this logic.

---

## Lifecycle: creating and deleting clips

### Clip creation (worker pipeline)

When a clip is created in the worker (`backend/crates/vclip-worker/src/clip_pipeline/clip.rs`):

1. The worker generates the clip file and computes `result.file_size_bytes`.
2. Clip metadata is written to Firestore via `ClipRepository`.
3. **Video-level totals**:
   - `VideoRepository::add_clip_size(video_id, size_bytes)` is called.
   - This reads the current `total_size_bytes` (0 if missing), adds the new size with `saturating_add`, and updates:
     - `video.total_size_bytes`
     - `video.updated_at`
4. **User-level totals** (optimistic concurrency):
   - A background task fetches the `users/{uid}` document via `FirestoreClient::get_document`.
   - It reads `total_storage_bytes` and `total_clips_count` from the document (defaulting to 0 if missing).
   - It increments both counters and writes them back using `update_document_with_precondition`:
     - `currentDocument.updateTime` is set to the document's `update_time`.
     - Only the specific fields `total_storage_bytes`, `total_clips_count`, and `updated_at` are updated via an `updateMask`.
   - If Firestore returns a **precondition failed** error (another writer updated the document first), the worker retries up to a fixed number of attempts.
   - All failures are logged with `warn!` but do not fail the clip rendering job itself.

This pattern avoids lost updates when many clips are produced concurrently for the same user.

### Clip deletion (single)

For deleting a single clip (`delete_clip` in `backend/crates/vclip-api/src/handlers/videos.rs`):

1. Ownership is verified via `UserService::user_owns_video`.
2. The handler tries to determine the clip size:
   - First by reading clip metadata from Firestore (`ClipRepository::list` and matching `filename`).
   - If Firestore lookup fails or doesn't contain the size, it falls back to listing objects from storage (R2) and matching on the key.
   - If no size can be determined, `clip_size_bytes` falls back to `0` and a warning is logged.
3. The clip and its thumbnail are deleted from storage (R2) via `state.storage.delete_clip`.
4. Clip metadata is removed via `ClipRepository::delete_by_filename`.
5. If a non-zero size is known:
   - `VideoRepository::subtract_clip_size(video_id, size_bytes)` is called to update `video.total_size_bytes`.
6. User-level totals are decremented via `UserService::subtract_storage(uid, size_bytes)`:
   - Uses `saturating_sub` for both `total_storage_bytes` and `total_clips_count` to avoid underflow.
   - Updates `updated_at` and persists the user document.
7. `refresh_clips_count` ensures `video.clips_count` remains accurate.

If some lookups fail and sizes are unknown, the system may temporarily drift until a reconciliation is run (see below).

### Bulk clip deletion and delete-all

Bulk delete paths (`bulk_delete_clips` and `delete_all_clips`) follow the same principles:

- Iterate over target clips, apply the same size detection logic, and update video/user counters.
- Track whether **any** clips had unknown sizes during the operation.
- If any unknown sizes were encountered, trigger a full storage recalculation for the user at the end (see next section).

### Video deletion

For video deletion (`delete_video`):

1. Ownership is verified.
2. The handler loads the video metadata to determine:
   - `video.total_size_bytes`
   - Number of clips for that video.
3. All files for the video are deleted from storage, and the video document is removed from Firestore.
4. If the video previously had a non-zero size or clips, `UserService::recalculate_storage(&uid)` is invoked to recompute user-level totals from scratch.

This guarantees that after a full video deletion, global user totals remain consistent even if some per-clip sizes were missing.

### Reconciliation and migration

`UserService::recalculate_storage(&uid)` provides a full reconciliation path:

- Iterates over all videos for the user via `VideoRepository::list`.
- For each video:
  - Loads all clips via `ClipRepository::list`.
  - Sums `file_size_bytes` for all clips to get the accurate total size.
  - Updates `video.total_size_bytes` via `VideoRepository::update_total_size`.
- Aggregates totals across all videos into `total_bytes` and `total_clips`.
- Writes `user.total_storage_bytes` and `user.total_clips_count`.

An admin-only endpoint (`POST /api/admin/users/:uid/storage/recalculate`) exposes this operation as part of the API (see below).

This reconciliation is intended for:

- **Migrations** from older schemas that did not track storage.
- **Self-healing** when unknown sizes or partial failures are detected.

---

## Quota enforcement

Quota enforcement is performed on the backend before expensive processing starts. The frontend mirrors the state for UX but the server is authoritative.

### Monthly clip quota

The monthly clip quota is based on:

- `UserService::get_monthly_usage(&uid) -> u32` – number of clips consumed in the current billing period.
- `UserService::get_plan_limits(&uid) -> PlanLimits` – pulls limits from Firestore and/or defaults.
- `UserService::validate_plan_limits(&uid, additional_clips)` – ensures `used + additional_clips` does not exceed `max_clips_per_month`.

Entry points that enforce clip quotas include:

- **Primary REST reprocessing endpoint** (`reprocess_scenes` in `handlers/videos.rs`)
  - Fetches `used` and `limits`.
  - If `used >= limits.max_clips_per_month`, returns `403 Forbidden` with a human-readable message.
  - Computes `total_clips` for the request and calls `validate_plan_limits` to ensure the new work will fit within the month.
- **WebSocket process/reprocess endpoints** (`handle_process_socket` and `handle_reprocess_socket` in `ws.rs`)
  - On connection setup, these handlers:
    - Fetch `used` and `limits`.
    - If the monthly clip quota is exceeded, send a `WsMessage::error` and terminate the connection early.

### Storage quota

Storage quota is enforced both at request-time and via a dedicated API:

- `UserService::get_storage_usage(&uid) -> StorageUsage`
- `StorageUsage::percentage()` and `StorageUsage::would_exceed(additional_bytes)` determine how close the user is to their plan's storage limit.

Server-side enforcement includes:

- **WebSocket process/reprocess** (`ws.rs`)
  - After clip quota checks, the server fetches `usage = get_storage_usage(&uid)`.
  - If `usage.percentage() >= 100.0`, a `WsMessage::error` is sent and the job is rejected.
- **REST reprocessing** (`reprocess_scenes`)
  - Fetches `storage_usage` and denies the request with `403 Forbidden` when `percentage >= 100.0`, instructing the user to delete clips or upgrade.

A dedicated REST API allows proactive checks and UI hints:

- `GET /api/storage/quota` (`get_storage_quota`):
  - Returns `StorageQuotaResponse` with fields:
    - `used_bytes`, `limit_bytes`, `total_clips`
    - `percentage`
    - `used_formatted`, `limit_formatted`, `remaining_formatted`
    - `plan` (plan id)
    - `is_near_limit` (≥ 80%)
    - `is_exceeded` (≥ 100%)
- `POST /api/storage/check` (`check_storage_quota`):
  - Request: `CheckQuotaRequest { size_bytes: u64 }`
    - `size_bytes` is clamped by `MAX_CLIP_SIZE_BYTES = 10 GB` to prevent abuse.
  - Response: `CheckQuotaResponse` with:
    - `allowed: bool`
    - `current_bytes`, `limit_bytes`, `requested_bytes`
    - `message` – human-readable explanation.

This "check" endpoint is designed for future upload flows where the client can ask "Would this upload exceed my storage?" before starting a large transfer.

### WebSocket rate limiting & plan checks

In addition to quotas, the WebSocket layer enforces:

- **Connection limits per user** (`MAX_CONCURRENT_CONNECTIONS_PER_USER = 3`).
- **Minimum time between jobs** (`MIN_JOB_INTERVAL = 5s`).

These limits live in `ws.rs` and are enforced via `UserConnectionTracker::try_acquire`. They are orthogonal to plan quotas but interact in practice: a user who is over quota will be denied quickly and will not tie up worker resources.

---

## API surface

The following APIs are relevant to plans and quotas.

### `GET /api/settings`

Handler: `get_settings` in `backend/crates/vclip-api/src/handlers/settings.rs`.

- Authenticated endpoint returning combined user settings and usage info.
- Response (`UserSettingsResponse`):
  - `settings: Map<String, serde_json::Value>` – user preferences and defaults.
  - `plan: String` – current plan id (`"free"`, `"pro"`, `"studio"`, etc.).
  - `max_clips_per_month: u32`
  - `clips_used_this_month: u32`
  - `role: Option<String>` – `"superadmin"` when applicable.
  - `storage: StorageInfo` – storage usage (mirrors `StorageQuotaResponse` without flags).

This is the primary endpoint consumed by the frontend for per-user usage display.

### `GET /api/storage/quota`

Handler: `get_storage_quota` in `handlers/storage.rs`.

- Authenticated endpoint returning `StorageQuotaResponse` (see above).
- Useful for UIs that want to show storage usage and limit without the rest of the settings payload.

### `POST /api/storage/check`

Handler: `check_storage_quota` in `handlers/storage.rs`.

- Authenticated endpoint to check whether a clip of a given size can be uploaded within the user's plan.
- Intended for future upload flows and not currently used by the core clip-processing pipeline.

### `POST /api/admin/users/:uid/storage/recalculate`

Handler: `recalculate_user_storage` in `handlers/admin.rs`.

- Admin-only (superadmin) endpoint for reconciliation and migrations.
- Recomputes `total_storage_bytes` and `total_clips_count` for the target user and returns:
  - `total_storage_bytes`
  - `total_storage_formatted`
  - `total_clips`
  - A human-readable `message` summarizing the result.

### WebSocket endpoints

The WebSocket endpoints (`handle_process_socket` and `handle_reprocess_socket` in `ws.rs`) are not directly exposed as REST paths in the docs but are reachable via the frontend through `wss://.../ws/process` and `wss://.../ws/reprocess` URLs.

They enforce:

- Connection limits and rate limits.
- Monthly clip quotas.
- Storage quotas.
- Plan gating for higher intelligent tiers (see `INTELLIGENT_TIERS_DESIGN.md`).

---

## Frontend UX and thresholds

The frontend displays plan and quota information in several places, using the backend as the source of truth.

### Shared storage helpers

`web/types/storage.ts` defines helpers used across UIs:

- `StorageInfo` – TypeScript mirror of the storage fields returned from the backend.
- `PLAN_STORAGE_LIMITS` – byte limits per plan, kept in sync with Rust constants.
- `formatBytes(bytes)` – formats raw bytes into `KB / MB / GB` strings.
- `calculateStoragePercentage(used, limit)` – returns a percentage capped at 100.
- `wouldExceedStorage(currentUsed, additionalBytes, limit)` – client-side helper for hypothetical checks.
- `parseSizeToBytes(sizeStr)` – converts human-readable `"1.5 MB"` strings back to bytes.

These helpers are used by multiple components to ensure consistent display of sizes and percentages.

### Settings page (`/settings`)

File: `web/app/settings/page.tsx`.

- Loads `SettingsResponse` from `/api/settings`.
- Displays:
  - Current plan id.
  - `clips_used_this_month / max_clips_per_month`.
  - Storage usage with a progress bar and labels:
    - `used_formatted / limit_formatted`.
    - `total_clips` and `remaining_formatted`.
  - Visual warnings when storage is high:
    - `isHighStorage` (≥ 80%).
    - Stronger warning when `storage.percentage >= 90`.

This gives users a quick overview of their usage without leaving the settings page.

### History page (`/history`)

File: `web/app/history/HistoryList.tsx`.

- Displays two usage sections in the sidebar:
  - **Monthly Clips** – clip quota usage.
  - **Storage** – storage quota usage when `planUsage.storage` is available.
- Shows progress bars and textual summaries:
  - `usagePercentage` for clips.
  - `storagePercentage` for storage.
- Visual states:
  - Normal, high usage, near limit, and over limit.
- When either clips or storage are over limit (≥ 100%), an "over limit" warning banner appears with a call-to-action to upgrade or delete clips.
- The video table includes a **Size** column when `v.total_size_formatted` is present, allowing users to identify large videos.

### Process page (`/process`)

File: `web/components/process/ProcessVideoInterface.tsx`.

- Fetches `UserSettings` (including `storage: StorageInfo`) from `/api/settings`.
- Maintains a local `quotaInfo` state with:
  - `clipsUsed`, `clipsLimit`.
  - `storageUsed`, `storageLimit`.
- Derived booleans:
  - `isOverClipQuota` – `clipsUsed >= clipsLimit`.
  - `isOverStorageQuota` – `storageUsed >= storageLimit`.
  - `isOverQuota` – either of the above.
- Effects on UX:
  - When `isOverQuota` is true:
    - A prominent banner explains that plan limits have been exceeded and links to:
      - `/pricing` (upgrade).
      - `/history` (manage/delete clips).
    - The main "Launch" button is disabled and visually marked as "Quota Exceeded" with an `AlertCircle` icon.
    - Attempts to start processing show a toast error and abort before opening a WebSocket.

This closely mirrors the server-side enforcement so users see **why** a job is being rejected.

### Scene explorer (`/history/[id]` – `SceneExplorer`)

File: `web/components/HistoryDetail/SceneExplorer.tsx`.

- Each scene shows an aggregate clip size badge when size data is available:
  - Uses `parseSizeToBytes(clip.size)` and `formatBytes(totalSceneBytes)` from `@/types/storage`.
- This helps users identify which scenes contribute most to storage usage.

### Pricing page (`/pricing`)

File: `web/app/pricing/page.tsx`.

- Marketing-facing representation of plans.
- Lists storage per plan in human-readable terms (`"1 GB storage"`, `"30 GB storage"`, `"150 GB storage"`).
- Must remain aligned with the backend constants in `vclip-models::plan` and the frontend `PLAN_STORAGE_LIMITS`.

---

## Operational notes

### Backfilling storage usage for existing users

For users that existed before storage tracking was introduced, `total_storage_bytes` and `total_clips_count` may initially be `0`. To backfill:

1. Ensure clip documents contain accurate `file_size_bytes` (this is the case for clips rendered by the current worker).
2. For each user, call the admin endpoint:
   - `POST /api/admin/users/:uid/storage/recalculate`.
   - This recomputes totals from all videos and clips and updates both video and user records.
3. Optionally, build a maintenance script that iterates all users and invokes this endpoint or the underlying service method.

### Data correctness and invariants

To keep quotas trustworthy:

- All clip creation flows must update **both** video and user totals.
- All clip deletion flows must:
  - Delete storage objects.
  - Delete Firestore clip metadata.
  - Adjust video/user totals or trigger `recalculate_storage` when sizes are unknown.
- Admin maintenance jobs should periodically check for anomalies (e.g. negative remaining storage, suspiciously high `total_clips_count`) and can use `recalculate_storage` as an automatic fix.

### Observability

Key log messages and metrics include:

- Storage updates after clip creation and deletion (info-level success, warn-level failures).
- Warnings when clip sizes are unknown or when Firestore preconditions fail repeatedly.
- Denials due to clip or storage quota being exceeded (HTTP 403 or WebSocket error messages).

Over time, these signals can be wired into dashboards and alerts to detect abuse or misconfiguration (see `analytics.md` and `logging-and-observability.md`).

---

## Operations runbook

### Bulk recalculate storage for all users

When migrating from a schema without storage tracking (or after a bug that caused drift), you may need to recalculate storage for every user in the system.

#### Option A – Admin endpoint loop (safest)

Use a simple script or one-liner that iterates all user UIDs and calls the admin endpoint. Example using `curl` and a file of UIDs:

```bash
# 1. Export user UIDs from Firestore (via console export or query)
#    Place them in users.txt, one UID per line.

# 2. Loop over each UID and call the recalculate endpoint
API_BASE="https://api.yourdomain.com"
AUTH_TOKEN="<superadmin_token>"

while read -r uid; do
  echo "Recalculating storage for $uid ..."
  curl -s -X POST "$API_BASE/api/admin/users/$uid/storage/recalculate" \
       -H "Authorization: Bearer $AUTH_TOKEN" | jq .
  sleep 0.2  # rate-limit yourself
done < users.txt
```

This uses the existing API and respects auth/rate limits.

#### Option B – Direct Firestore batch (faster, riskier)

For large user bases where HTTP overhead is a concern, you can invoke `UserService::recalculate_storage` directly in a maintenance job:

1. Build a small Rust binary or extend an existing admin CLI.
2. Query `users` collection for all UIDs.
3. For each user, call `user_service.recalculate_storage(&uid).await`.
4. Log success/failure per user.

This bypasses the HTTP layer but runs inside the same trust boundary, so production credentials are required. Only use when you control the execution environment.

#### Verifying correctness

After a bulk recalculation:

1. Spot-check a few users by comparing `user.total_storage_bytes` to a manual sum of their clips' `file_size_bytes`.
2. Query for anomalies:
   - Users with `total_storage_bytes < 0` (should never happen with `saturating_sub`).
   - Users with `total_clips_count = 0` but `total_storage_bytes > 0` (indicates mismatch).
3. Watch logs for warnings during normal clip creation/deletion to catch future drift early.

---

## Summary

- **Plan tiers and limits** are defined in `vclip-models::plan` and can be overridden per-plan in Firestore.
- **Storage and clip counters** are tracked at clip, video, and user levels and updated incrementally by the worker and API handlers.
- **Quotas** (monthly clips and storage) are enforced on the backend before work starts and surfaced to the frontend for clear UX.
- **Reconciliation** via `recalculate_storage` keeps counters correct over time, especially during migrations.
- **Frontend UIs** (settings, history, process, scene explorer, pricing) consume these APIs to give users visibility into their usage and upgrade paths.
