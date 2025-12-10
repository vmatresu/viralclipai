# Video Delivery Layer

Secure, production-grade video delivery for Viral Clip AI using Cloudflare R2 and presigned URLs.

## Overview

This document describes the video delivery layer that provides secure playback, download, and sharing of clips. The layer follows a **security-first** approach with the principle of least privilege.

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Frontend      │────▶│   Rust API       │────▶│   Cloudflare    │
│   (Next.js)     │     │   (Axum)         │     │   R2            │
└─────────────────┘     └──────────────────┘     └─────────────────┘
        │                        │
        │ POST /api/clips/{id}/  │ Generate presigned URL
        │     play-url           │ (short-lived)
        │◀───────────────────────│
        │                        │
        │ <video src={url}>      │
        └────────────────────────────────────────▶│ Direct R2 fetch
```

### Primary Pattern: Presigned URLs

The primary delivery pattern uses **S3-compatible presigned URLs** directly to R2:

1. **Frontend** requests a playback/download URL via authenticated API call
2. **Backend** validates ownership, generates a short-lived presigned URL
3. **Frontend** uses the URL directly in `<video>` elements or for downloads
4. **R2** serves the content directly to the client

**Advantages:**

- No additional infrastructure (Workers)
- Built-in security via S3 signature
- Short-lived tokens (15-60 minutes)
- Works with existing `vclip-storage` crate

### Worker-Fronted Pattern

The system now fully supports Worker-fronted delivery with stateless tokens:

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Frontend      │────▶│   CF Worker      │────▶│   R2 Binding    │
│                 │     │   (Gateway)      │     │                 │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               │ HMAC-signed token
                               │ in query param
```

To enable Worker delivery, set:

```bash
CDN_WORKER_URL=https://cdn.viralclipai.io
DELIVERY_SIGNING_SECRET=your-32-byte-secret-key
PREFER_WORKER_DELIVERY=true
```

## API Endpoints

### Playback URL

```http
POST /api/clips/{clip_id}/play-url
Authorization: Bearer {token}
```

**Response:**

```json
{
  "url": "https://xxx.r2.cloudflarestorage.com/...?X-Amz-Signature=...",
  "expires_at": "2024-01-01T12:15:00Z",
  "expires_in_secs": 900,
  "content_type": "video/mp4",
  "clip": {
    "clip_id": "clip-abc123",
    "filename": "clip_01_1_intro_split.mp4",
    "title": "Introduction",
    "duration_seconds": 45.5,
    "file_size_bytes": 12345678
  }
}
```

### Download URL

```http
POST /api/clips/{clip_id}/download-url
Authorization: Bearer {token}
Content-Type: application/json

{
  "filename": "my-custom-name.mp4"
}
```

**Response:** Same as playback URL, with `Content-Disposition` header for download.

### Thumbnail URL

```http
POST /api/clips/{clip_id}/thumbnail-url
Authorization: Bearer {token}
```

**Response:**

```json
{
  "url": "https://xxx.r2.cloudflarestorage.com/...?X-Amz-Signature=...",
  "expires_at": "2024-01-01T12:15:00Z",
  "expires_in_secs": 900,
  "content_type": "image/jpeg"
}
```

### Create Share Link

```http
POST /api/clips/{clip_id}/share
Authorization: Bearer {token}
Content-Type: application/json

{
  "access_level": "view_playback",
  "expires_in_hours": 24,
  "watermark_enabled": false
}
```

**Constraints:**

- `expires_in_hours` max: 720 (30 days)

**Response:**

```json
{
  "share_url": "https://viralclipai.io/c/abc123xyz",
  "share_slug": "abc123xyz",
  "access_level": "view_playback",
  "expires_at": "2024-01-02T12:00:00Z",
  "watermark_enabled": false,
  "created_at": "2024-01-01T12:00:00Z"
}
```

**Access Levels:**

- `none` - No public access (owner only)
- `view_playback` - Playback only
- `download` - Playback and download allowed

### Revoke Share Link

```http
DELETE /api/clips/{clip_id}/share
Authorization: Bearer {token}
```

**Response:** `204 No Content`

### Public Share Resolution

```http
GET /c/{share_slug}
```

**Responses:**

- `302 Found` - Redirect to a fresh presigned/worker URL for playback
- `404 Not Found` - Share slug does not exist
- `410 Gone` - Share has been revoked or expired

**Headers:**

- `Location: {delivery_url}` - Short-lived playback URL
- `Cache-Control: private, max-age=60` - Prevent long caching so revocation takes effect quickly

## CORS Configuration

### R2 Bucket CORS (JSON)

Apply this CORS configuration to your R2 bucket:

```json
[
  {
    "AllowedOrigins": ["https://app.viralclipai.io", "https://viralclipai.io"],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["Range", "Origin", "Accept", "Content-Type"],
    "ExposeHeaders": [
      "Content-Length",
      "Content-Type",
      "Accept-Ranges",
      "Content-Range"
    ],
    "MaxAgeSeconds": 86400
  }
]
```

### Development CORS

For local development, add localhost:

```json
[
  {
    "AllowedOrigins": [
      "https://app.viralclipai.io",
      "https://viralclipai.io",
      "http://localhost:3000"
    ],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["Range", "Origin", "Accept", "Content-Type"],
    "ExposeHeaders": [
      "Content-Length",
      "Content-Type",
      "Accept-Ranges",
      "Content-Range"
    ],
    "MaxAgeSeconds": 86400
  }
]
```

### Applying CORS via Wrangler

```bash
# Save config to cors.json, then:
wrangler r2 bucket cors put viralclipai-videos --file cors.json
```

## Environment Variables

Add these to your `.env` for the API:

```bash
# Delivery URL expiry (seconds)
PLAYBACK_URL_EXPIRY_SECS=900      # 15 minutes (default)
DOWNLOAD_URL_EXPIRY_SECS=300      # 5 minutes (default)

# For Worker-fronted delivery (optional, future)
CDN_WORKER_URL=https://cdn.viralclipai.io
DELIVERY_SIGNING_SECRET=your-32-byte-secret-key
PREFER_WORKER_DELIVERY=false

# Public app URL for share links
PUBLIC_APP_URL=https://viralclipai.io
```

## Security Considerations

### URL as Bearer Token

Treat presigned URLs as **bearer tokens**:

- They grant access to anyone who has them
- They may leak in logs, referrers, chat messages
- Always use short expiry (15 minutes for playback, 5 for download)

### Bucket Access Model

1. **R2 Bucket**: Private (no public list, no anonymous write)
2. **API Access**: Via `vclip-storage::R2Client` with credentials
3. **Public Access**: Only via presigned URLs or Worker with signed tokens

### Token Signing (Worker Pattern)

The `DeliveryToken` uses HMAC-SHA256 and includes the R2 key for stateless Worker delivery:

```rust
pub struct DeliveryToken {
    pub cid: String,       // Clip ID
    pub uid: String,       // User ID
    pub scope: String,     // "play" | "dl" | "thumb"
    pub exp: u64,          // Expiry timestamp
    pub r2_key: Option<String>, // R2 object key for stateless delivery
    pub share: Option<bool>,
    pub wm: Option<bool>,  // Watermark flag
}
```

The `r2_key` field enables fully stateless Worker delivery - the Worker trusts this key because the token is HMAC-signed by the backend.

Signed as: `base64(json(token)).base64(hmac-sha256(payload, secret))`

## Integration

### Frontend Usage

```typescript
import {
  getPlaybackUrl,
  getDownloadUrl,
  createShare,
  downloadClip,
  playInNewTab,
} from "@/lib/clipDelivery";

// Playback in video element
const { url } = await getPlaybackUrl(clipId, authToken);
videoElement.src = url;

// Play in new tab
await playInNewTab(clipId, authToken);

// Download
await downloadClip(clipId, authToken, "my-clip.mp4");

// Share
const share = await createShare(clipId, authToken, {
  access_level: "view_playback",
  expires_in_hours: 24,
});
console.log("Share URL:", share.share_url);
```

### Backend Crate Structure

```
vclip-storage/src/
  delivery.rs     # DeliveryUrlGenerator, DeliveryToken
  client.rs       # R2Client with presign_get()

vclip-models/src/
  share.rs        # ShareConfig, ShareAccessLevel, ShareResponse

vclip-firestore/src/
  repos.rs        # ShareRepository (dual-document pattern)

vclip-api/src/
  handlers/
    clip_delivery.rs  # API handlers (create/revoke/resolve share)
  routes.rs           # Route definitions

workers/clip-gateway/src/
  auth.ts         # DeliveryToken verification
  index.ts        # Worker entry point with stateless R2 delivery
```

## Migration from Direct CDN URLs

If you previously used `R2_PUBLIC_URL` for direct CDN access:

1. **Remove** public bucket access in R2 settings
2. **Update** frontend to use the new API endpoints
3. **Set** `R2_PUBLIC_URL` to empty (or remove it)
4. **Apply** the CORS configuration above

Existing clips continue to work; only the URL delivery method changes.

## Troubleshooting

### CORS Errors

If you see CORS errors in the browser:

1. Verify R2 CORS config includes your origin
2. Check that `Range` header is in `AllowedHeaders`
3. Ensure `Accept-Ranges` is in `ExposeHeaders`

### URL Expired

If playback fails with 403:

1. Check `expires_in_secs` in response
2. Request a fresh URL before expiry
3. Consider increasing `PLAYBACK_URL_EXPIRY_SECS`

### Share Link Not Working

If `/c/{slug}` returns an error:

1. **404 Not Found**: The share slug doesn't exist in Firestore
2. **410 Gone**: The share was revoked (`disabled_at` set) or expired (`expires_at` passed)
3. Verify share was created successfully by checking the `share_url` in the response
4. Check Firestore for the `share_slugs/{slug}` document

## Implementation Status

| Feature                 | Status         | Notes                                          |
| ----------------------- | -------------- | ---------------------------------------------- |
| Playback URLs           | ✅ Implemented | Presigned URLs to R2                           |
| Download URLs           | ✅ Implemented | With Content-Disposition                       |
| Thumbnail URLs          | ✅ Implemented | Presigned URLs to R2                           |
| Share Link Creation     | ✅ Implemented | Persisted to Firestore (dual-document pattern) |
| Share Link Revocation   | ✅ Implemented | Deletes slug index, marks config disabled      |
| Share Link Resolution   | ✅ Implemented | 302 redirect with short-lived delivery URL     |
| Worker-Fronted Delivery | ✅ Implemented | Stateless with r2_key embedded in token        |

## Data Model

### Dual-Document Pattern

Share links use a dual-document pattern for optimal read/write performance:

1. **Config Document**: `users/{user_id}/videos/{video_id}/clips/{clip_id}/shares/config`

   - Full ShareConfig with all settings
   - Scoped to user for security

2. **Slug Index Document**: `share_slugs/{share_slug}`
   - Minimal index for fast public lookup
   - Contains: user_id, video_id, clip_id, access_level, expires_at, disabled_at

### Two-Tier Expiry

- **Share Link**: Up to 30 days (720 hours)
- **Delivery Token**: 1 hour (for each redirect)

This means the share link itself can be long-lived, but each playback URL is short-lived.

## Future Enhancements

1. **Watermarking**: Burn user/share info into video on-the-fly
2. **Analytics**: Track playback counts per clip/share
3. **Rate Limiting**: Per-user bandwidth limits
4. **Geo-Restrictions**: Block/allow by country
5. **Batch Transactions**: Use Firestore batch writes for stronger atomicity
