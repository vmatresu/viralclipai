# Pricing & Credits Implementation v1

## Overview

This document describes the credit-based pricing model implemented in ViralClipAI.

## Credit System

Credits are the primary unit for tracking usage. Unlike simple clip counts, credits allow for variable pricing based on processing complexity.

### Credit Costs by Feature

| Feature | Credits |
|---------|---------|
| **Video Analysis** (get scenes) | 3 |
| **Static styles** (DetectionTier::None) | 10 per clip |
| **Basic styles** (DetectionTier::Basic - YuNet face detection) | 10 per clip |
| **Smart styles** (MotionAware/SpeakerAware) | 20 per clip |
| **Premium styles** (Cinematic) | 30 per clip |
| **Streamer / StreamerSplit** | 10 per scene |
| **Scene originals download** | 5 per scene |
| **Silent remover add-on** | +5 per scene |
| **Object detection add-on** (Cinematic) | +10 |

### Monthly Credits by Plan

| Plan | Credits/Month | Approx. Basic Clips |
|------|---------------|---------------------|
| Free | 200 | ~20 |
| Pro | 4,000 | ~400 |
| Studio | 12,000 | ~1,200 |

### Storage Limits

| Plan | Storage |
|------|---------|
| Free | 1 GB |
| Pro | 30 GB |
| Studio | 150 GB |

## Plan Feature Gating

### Detection Tier Access

| Plan | Allowed Tiers |
|------|---------------|
| Free | None (Static), Basic (Smart Face) |
| Pro | None, Basic, MotionAware, SpeakerAware |
| Studio | All including Cinematic |

### Other Features

| Feature | Free | Pro | Studio |
|---------|------|-----|--------|
| Watermark | Yes | No | No |
| API Access | No | No | Yes |
| Channel Monitoring | No | No | 2 channels |
| Priority Processing | No | Yes | Yes |
| Connected Accounts | 1 | 3 | 10 |

## Firestore Plan Documents

Plan configuration is stored in `plans/{plan_id}` documents. The system falls back to hardcoded defaults if the document is missing.

### plans/free

```json
{
  "id": "free",
  "name": "Free",
  "limits": {
    "monthly_credits_included": 200,
    "storage_limit_bytes": 1073741824,
    "max_highlights_per_video": 3,
    "max_styles_per_video": 2,
    "watermark_exports": true,
    "api_access": false,
    "channel_monitoring_included": 0,
    "connected_social_accounts_limit": 1,
    "priority_processing": false
  },
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

### plans/pro

```json
{
  "id": "pro",
  "name": "Pro",
  "price_monthly": 2900,
  "limits": {
    "monthly_credits_included": 4000,
    "storage_limit_bytes": 32212254720,
    "max_highlights_per_video": 10,
    "max_styles_per_video": 5,
    "watermark_exports": false,
    "api_access": false,
    "channel_monitoring_included": 0,
    "connected_social_accounts_limit": 3,
    "priority_processing": true
  },
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

### plans/studio

```json
{
  "id": "studio",
  "name": "Studio",
  "price_monthly": 9900,
  "limits": {
    "monthly_credits_included": 12000,
    "storage_limit_bytes": 161061273600,
    "max_highlights_per_video": 25,
    "max_styles_per_video": 10,
    "watermark_exports": false,
    "api_access": true,
    "channel_monitoring_included": 2,
    "connected_social_accounts_limit": 10,
    "priority_processing": true
  },
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

## User Document Fields

The following fields are used on user documents:

```json
{
  "uid": "user_id",
  "plan": "free",
  "credits_used_this_month": 0,
  "usage_reset_month": "2024-12",
  "total_storage_bytes": 0,
  "total_clips_count": 0
}
```

- `credits_used_this_month`: Monthly credit usage counter
- `usage_reset_month`: Current billing month (YYYY-MM format)
- `total_storage_bytes`: Total storage used across all clips
- `total_clips_count`: Number of clips stored

## API Changes

### GET /api/settings

Response format:

```json
{
  "plan": "free",
  "monthly_credits_limit": 200,
  "credits_used_this_month": 0,
  "features": {
    "watermark_exports": true,
    "api_access": false,
    "channel_monitoring": false,
    "max_clip_length_seconds": 90,
    "priority_processing": false
  },
  "storage": {
    "used_bytes": 0,
    "limit_bytes": 1073741824,
    "total_clips": 0,
    "percentage": 0,
    "used_formatted": "0 B",
    "limit_formatted": "1.00 GB",
    "remaining_formatted": "1.00 GB"
  }
}
```

### Admin Endpoints

- `GET /api/admin/users` - Returns `credits_used_this_month` and `monthly_credits_limit`
- `PUT /api/admin/users/:uid/usage` - Accepts `credits_used` to set credit usage

## Migration Script

For existing users, run the migration script to reset to credits:

```bash
# Using Node.js
GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json node backend/scripts/migrate-users-to-credits.js
```

This script:
1. Sets `credits_used_this_month` to 0 (full quota available)
2. Updates `usage_reset_month` to current month
3. Removes legacy `clips_used_this_month` field

### For Existing Plan Documents

If Firestore plan documents don't have the new fields, the system uses hardcoded defaults from `PlanTier::monthly_credits()` and related methods.

## Security & Concurrency

### Atomic Credit Reservation

Credit reservation uses Firestore's optimistic locking pattern to prevent race conditions:

1. Read user's current credits and `updated_at` timestamp
2. Calculate new credits value
3. Attempt update with precondition on `updated_at`
4. If precondition fails (concurrent update), retry up to 3 times with exponential backoff

This ensures credits are never double-charged or over-deducted even with concurrent requests.

### URL Validation

All video URLs are validated using the `security::validate_video_url()` function which:
- Enforces HTTPS protocol
- Validates against a domain whitelist (YouTube, Vimeo, TikTok, etc.)
- Blocks internal IPs and cloud metadata endpoints (SSRF protection)
- Limits URL length to prevent DoS attacks

### Input Sanitization

User inputs (prompts, titles) are sanitized using `security::sanitize_string()` which removes control characters while preserving legitimate newlines and tabs.

## Important Notes

1. **Credits are NOT refunded** when clips are deleted
2. **Credits are charged upfront** before processing begins
3. **Analysis costs credits** (3 credits for video analysis)
4. **Failed jobs still consume credits** - no refund on failure
5. **Monthly reset** happens based on `usage_reset_month` field matching current YYYY-MM

## Testing Locally

1. Start the backend: `cargo run -p vclip-api`
2. Check user settings: `GET /api/settings` with auth token
3. Process a video to see credits being charged
4. Check Firestore `users/{uid}` document for `credits_used_this_month`

## File Changes Summary

### Backend

- `backend/crates/vclip-models/src/plan.rs` - Credit constants, PlanLimits struct, tier methods
- `backend/crates/vclip-models/src/style.rs` - `credit_cost()` uses detection tier
- `backend/crates/vclip-api/src/security.rs` - URL validation, input sanitization
- `backend/crates/vclip-api/src/services/user.rs` - Credit tracking, atomic reservation with Firestore optimistic locking
- `backend/crates/vclip-api/src/handlers/videos.rs` - Credit-based quota enforcement
- `backend/crates/vclip-api/src/handlers/analysis.rs` - SSRF-safe URL validation, style-based cost estimation
- `backend/crates/vclip-api/src/handlers/settings.rs` - Returns credits and feature flags (no legacy clips)
- `backend/crates/vclip-api/src/handlers/admin.rs` - Credits-based user management

### Frontend

- `web/components/landing/PricingSection.tsx` - Updated plan info
- `web/app/history/components/UsageCard.tsx` - Shows credits usage
- `web/app/history/HistoryList.tsx` - Credits-based usage display
- `web/app/history/types.ts` - Credits-only types (no legacy clips)
- `web/components/process/ProcessVideoInterface.tsx` - Credits quota checks

### Scripts

- `backend/scripts/migrate-users-to-credits.js` - Migration script for existing users
- `backend/scripts/migrate-users-to-credits.sh` - Shell script alternative
