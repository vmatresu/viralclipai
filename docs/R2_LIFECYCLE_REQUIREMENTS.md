# R2 Lifecycle Configuration Requirements

This document describes the required R2 bucket lifecycle rules for Viral Clip AI to function correctly.

## Overview

Viral Clip AI uses Cloudflare R2 for video storage with different retention policies:

| Storage Type      | R2 Path Pattern                           | Retention | Billable |
| ----------------- | ----------------------------------------- | --------- | -------- |
| **Styled Clips**  | `clips/{user_id}/{video_id}/styled/*.mp4` | Permanent | ✅ Yes   |
| **Raw Segments**  | `clips/{user_id}/{video_id}/raw/*.mp4`    | 7 days    | ❌ No    |
| **Source Videos** | `sources/{user_id}/{video_id}/source.mp4` | 24 hours  | ❌ No    |
| **Neural Cache**  | `{user_id}/{video_id}/neural/*.json.gz`   | 7 days    | ❌ No    |

## Required Lifecycle Rules

### 1. Source Video Expiration (24 hours)

Source videos are cached temporarily to speed up reprocessing. They must be automatically deleted after 24 hours.

**Rule Configuration:**

```json
{
  "rules": [
    {
      "id": "expire-source-videos-24h",
      "enabled": true,
      "prefix": "sources/",
      "expiration": {
        "days": 1
      }
    }
  ]
}
```

**Cloudflare Dashboard Steps:**

1. Go to R2 → Your Bucket → Settings → Lifecycle Rules
2. Add rule with prefix `sources/`
3. Set expiration to 1 day
4. Save

### 2. Raw Segment Expiration (7 days)

Raw segments are cached to speed up re-styling. They can be safely deleted after 7 days as they can be re-extracted from the source video.

**Rule Configuration:**

```json
{
  "id": "expire-raw-segments-7d",
  "enabled": true,
  "prefix": "clips/",
  "suffix": "/raw/",
  "expiration": {
    "days": 7
  }
}
```

**Note:** Cloudflare R2 lifecycle rules currently only support prefix matching. For more granular control, consider a cleanup worker.

### 3. Neural Cache Expiration (7 days)

Neural analysis results are versioned and can be safely expired as they can be recomputed.

**Rule Configuration:**

```json
{
  "id": "expire-neural-cache-7d",
  "enabled": true,
  "suffix": "/neural/",
  "expiration": {
    "days": 7
  }
}
```

## Alternative: Cleanup Worker

If R2 lifecycle rules don't provide sufficient granularity (e.g., suffix matching), deploy a Cloudflare Worker for cleanup:

```typescript
// workers/r2-cleanup/src/index.ts
export default {
  async scheduled(event: ScheduledEvent, env: Env) {
    const bucket = env.VCLIP_BUCKET;
    const now = Date.now();

    // Cleanup expired source videos (24h)
    await cleanupPrefix(bucket, "sources/", 24 * 60 * 60 * 1000, now);

    // Cleanup expired raw segments (7d)
    await cleanupPattern(
      bucket,
      /clips\/.*\/raw\//,
      7 * 24 * 60 * 60 * 1000,
      now
    );

    // Cleanup expired neural cache (7d)
    await cleanupPattern(bucket, /.*\/neural\//, 7 * 24 * 60 * 60 * 1000, now);
  },
};
```

**Worker Schedule:** Run hourly via Cloudflare Cron Triggers.

## Firestore Expiration Tracking

The application tracks expiration in Firestore for source videos:

```
users/{user_id}/videos/{video_id}:
  source_video_status: "ready" | "expired" | "pending" | "downloading" | "failed"
  source_video_r2_key: "sources/{user_id}/{video_id}/source.mp4"
  source_video_expires_at: <timestamp>
```

The worker code checks `source_video_expires_at` before attempting R2 downloads and marks status as "expired" if past TTL.

## Verification

After configuring lifecycle rules:

1. Upload a test file to `sources/test/test/source.mp4`
2. Wait 25+ hours
3. Verify the file is automatically deleted
4. Check Cloudflare R2 dashboard for lifecycle execution logs

## Important Notes

- **Do NOT** set lifecycle rules on `clips/*/styled/` - these are permanent user-owned clips
- Lifecycle rules run asynchronously; actual deletion may be delayed up to 24h after expiration
- Firestore metadata remains after R2 deletion; application handles "not found" gracefully
- Storage accounting (`storage_accounting/{user_id}`) is NOT decremented on lifecycle deletion - this is intentional for audit purposes
