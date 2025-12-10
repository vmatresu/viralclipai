# Clip Gateway Worker

Cloudflare Worker for secure video delivery via `cdn.viralclipai.io`.

## Overview

This Worker validates HMAC-signed tokens and streams clips from R2.
It's an **optional** enhancement for Worker-fronted delivery with edge caching.

The primary delivery pattern uses presigned URLs directly to R2, which works without this Worker.

## Routes

- `GET /v/{clip_id}?sig={signed_token}` - Video playback
- `GET /t/{clip_id}?sig={signed_token}` - Thumbnail
- `GET /health` - Health check

## Setup

### 1. Install Dependencies

```bash
cd workers/clip-gateway
npm install
```

### 2. Configure Secrets

```bash
# Set the HMAC signing secret (must match backend DELIVERY_SIGNING_SECRET)
wrangler secret put SIGNING_SECRET
```

### 3. Deploy

```bash
wrangler deploy
```

### 4. Configure Custom Domain (optional)

Uncomment and update the routes in `wrangler.toml`:

```toml
routes = [
  { pattern = "cdn.viralclipai.io/*", zone_name = "viralclipai.io" }
]
```

### 5. Enable in Backend

Set these environment variables in the backend:

```bash
CDN_WORKER_URL=https://cdn.viralclipai.io
DELIVERY_SIGNING_SECRET=your-32-byte-secret-key
PREFER_WORKER_DELIVERY=true
```

## Token Format

Tokens are HMAC-SHA256 signed JSON payloads:

```json
{
  "cid": "clip-123",
  "uid": "user-456",
  "scope": "play",
  "exp": 1704110400,
  "share": false,
  "wm": false
}
```

Signed format: `base64(json).base64(hmac-sha256-signature)`

## Local Development

```bash
wrangler dev
```

## Implementation Status

This Worker is a **minimal implementation** that demonstrates the token validation
and R2 streaming pattern. Before production use:

1. **Implement `resolveR2Key()`**: Map clip IDs to R2 keys via KV/D1 or encode in tokens
2. **Add metrics/logging**: Track request counts, latency, errors
3. **Add rate limiting**: Per-IP or per-token limits
4. **Test thoroughly**: Range requests, CORS, error cases

For now, use the **presigned URL pattern** which doesn't require this Worker.
