/**
 * Clip Gateway Worker
 *
 * Cloudflare Worker for secure video delivery via cdn.viralclipai.io.
 * Validates HMAC-signed tokens and streams clips from R2.
 *
 * Routes:
 * - GET /v/{clip_id}?sig={signed_token} - Video playback
 * - GET /t/{clip_id}?sig={signed_token} - Thumbnail
 *
 * Required secrets:
 * - SIGNING_SECRET: HMAC-SHA256 signing key (32+ bytes)
 *
 * Required bindings:
 * - CLIPS_BUCKET: R2 bucket binding
 */

import { DeliveryToken, verifyTokenAsync } from "./auth";
import { corsHeaders, handleOptions } from "./cors";

export interface Env {
  CLIPS_BUCKET: R2Bucket;
  SIGNING_SECRET: string;
  ALLOWED_ORIGINS: string;
}

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext
  ): Promise<Response> {
    // Handle CORS preflight
    if (request.method === "OPTIONS") {
      return handleOptions(request, env);
    }

    const url = new URL(request.url);
    const path = url.pathname;

    // Route: /v/{clip_id} - Video
    if (path.startsWith("/v/")) {
      return handleVideo(request, env, ctx, path.slice(3));
    }

    // Route: /t/{clip_id} - Thumbnail
    if (path.startsWith("/t/")) {
      return handleThumbnail(request, env, ctx, path.slice(3));
    }

    // Health check
    if (path === "/health") {
      return new Response("OK", { status: 200 });
    }

    return new Response("Not Found", { status: 404 });
  },
};

/**
 * Handle video playback requests.
 */
async function handleVideo(
  request: Request,
  env: Env,
  _ctx: ExecutionContext,
  clipId: string
): Promise<Response> {
  const url = new URL(request.url);
  const sig = url.searchParams.get("sig");

  if (!sig) {
    console.log("[handleVideo] Missing signature for clip:", clipId);
    return new Response("Missing signature", { status: 401 });
  }

  // Verify token
  const token = await verifyTokenAsync(sig, env.SIGNING_SECRET);
  if (!token) {
    console.log("[handleVideo] Invalid or expired token for clip:", clipId);
    return new Response("Invalid or expired signature", { status: 403 });
  }

  // Verify clip ID matches token
  if (token.cid !== clipId) {
    console.log("[handleVideo] Token clip ID mismatch:", {
      tokenCid: token.cid,
      requestCid: clipId,
    });
    return new Response("Token clip ID mismatch", { status: 403 });
  }

  // Verify scope
  if (token.scope !== "play" && token.scope !== "dl") {
    console.log("[handleVideo] Invalid scope for video:", token.scope);
    return new Response("Invalid token scope for video", { status: 403 });
  }

  // Resolve R2 key from token
  const r2Key = resolveR2Key(token, false);
  if (!r2Key) {
    console.log("[handleVideo] No r2_key in token for clip:", clipId);
    return new Response("Clip not found", { status: 404 });
  }

  // Fetch from R2 with error handling
  let object: R2ObjectBody | null;
  try {
    object = await env.CLIPS_BUCKET.get(r2Key, {
      range: parseRangeHeader(request),
    });
  } catch (err) {
    console.error("[handleVideo] R2 fetch error:", err);
    return new Response("Internal server error", { status: 500 });
  }

  if (!object) {
    console.log("[handleVideo] Object not found in R2:", r2Key);
    return new Response("Clip not found in storage", { status: 404 });
  }

  // Build response headers
  const headers = new Headers();
  headers.set("Content-Type", "video/mp4");
  headers.set("Accept-Ranges", "bytes");
  headers.set("Content-Length", String(object.size));

  // Add Content-Disposition for download scope
  if (token.scope === "dl") {
    headers.set("Content-Disposition", `attachment; filename="${clipId}.mp4"`);
  }

  // Add cache headers for public clips
  if (token.share) {
    headers.set("Cache-Control", "public, max-age=3600");
  } else {
    headers.set("Cache-Control", "private, no-store");
  }

  // Add CORS headers
  Object.entries(corsHeaders(request, env)).forEach(([k, v]) =>
    headers.set(k, v)
  );

  // Handle range requests
  const rangeHeader = request.headers.get("Range");
  if (rangeHeader && object.range) {
    headers.set("Content-Range", formatContentRange(object.range, object.size));
    return new Response(object.body, { status: 206, headers });
  }

  return new Response(object.body, { status: 200, headers });
}

/**
 * Handle thumbnail requests.
 */
async function handleThumbnail(
  request: Request,
  env: Env,
  _ctx: ExecutionContext,
  clipId: string
): Promise<Response> {
  const url = new URL(request.url);
  const sig = url.searchParams.get("sig");

  if (!sig) {
    console.log("[handleThumbnail] Missing signature for clip:", clipId);
    return new Response("Missing signature", { status: 401 });
  }

  // Verify token
  const token = await verifyTokenAsync(sig, env.SIGNING_SECRET);
  if (!token) {
    console.log("[handleThumbnail] Invalid or expired token for clip:", clipId);
    return new Response("Invalid or expired signature", { status: 403 });
  }

  // Verify clip ID matches
  if (token.cid !== clipId) {
    console.log("[handleThumbnail] Token clip ID mismatch:", {
      tokenCid: token.cid,
      requestCid: clipId,
    });
    return new Response("Token clip ID mismatch", { status: 403 });
  }

  // Verify scope
  if (token.scope !== "thumb" && token.scope !== "play") {
    console.log("[handleThumbnail] Invalid scope for thumbnail:", token.scope);
    return new Response("Invalid token scope for thumbnail", { status: 403 });
  }

  // Resolve thumbnail key from token
  const r2Key = resolveR2Key(token, true);
  if (!r2Key) {
    console.log("[handleThumbnail] No r2_key in token for clip:", clipId);
    return new Response("Thumbnail not found", { status: 404 });
  }

  // Fetch from R2 with error handling
  let object: R2ObjectBody | null;
  try {
    object = await env.CLIPS_BUCKET.get(r2Key);
  } catch (err) {
    console.error("[handleThumbnail] R2 fetch error:", err);
    return new Response("Internal server error", { status: 500 });
  }

  if (!object) {
    console.log("[handleThumbnail] Object not found in R2:", r2Key);
    return new Response("Thumbnail not found in storage", { status: 404 });
  }

  const headers = new Headers();
  headers.set("Content-Type", "image/jpeg");
  headers.set("Cache-Control", "public, max-age=86400"); // Thumbnails can be cached longer

  // Add CORS headers
  Object.entries(corsHeaders(request, env)).forEach(([k, v]) =>
    headers.set(k, v)
  );

  return new Response(object.body, { status: 200, headers });
}

/**
 * Resolve R2 key from token.
 *
 * Uses the r2_key field embedded in the token for stateless delivery.
 * The backend signs the token with HMAC-SHA256, so we trust the r2_key.
 *
 * @param token - Verified delivery token
 * @param thumbnail - If true, resolve thumbnail key instead of video
 * @returns R2 object key or null if not available
 */
function resolveR2Key(token: DeliveryToken, thumbnail = false): string | null {
  // The r2_key is embedded in the token by the backend
  if (!token.r2_key) {
    console.warn(
      "[resolveR2Key] Token does not contain r2_key - legacy token or misconfiguration"
    );
    return null;
  }

  // For thumbnails with scope "thumb", the backend already provides the correct
  // thumbnail R2 key directly in the token. Only derive the key for legacy tokens
  // or when the scope is "play" (video playback that also needs thumbnail).
  if (thumbnail) {
    // If scope is "thumb", the r2_key IS the thumbnail key (backend provides it directly)
    if (token.scope === "thumb") {
      return token.r2_key;
    }
    // For "play" scope tokens requesting thumbnail, derive from video key
    // Strip any extension (handles .mp4, .mov, .mkv, etc.) and append _thumb.jpg
    // e.g., users/uid/video/clips/clip.mp4 -> users/uid/video/clips/clip_thumb.jpg
    // e.g., users/uid/video/clips/clip.MOV -> users/uid/video/clips/clip_thumb.jpg
    return token.r2_key.replace(/\.[^/.]+$/, "") + "_thumb.jpg";
  }

  return token.r2_key;
}

/**
 * Parse Range header for partial content requests.
 */
function parseRangeHeader(request: Request): R2Range | undefined {
  const range = request.headers.get("Range");
  if (!range) return undefined;

  const match = range.match(/bytes=(\d+)-(\d*)/);
  if (!match) return undefined;

  const start = parseInt(match[1], 10);
  const end = match[2] ? parseInt(match[2], 10) : undefined;

  if (end !== undefined) {
    return { offset: start, length: end - start + 1 };
  }
  return { offset: start };
}

/**
 * Format Content-Range header for 206 responses.
 */
function formatContentRange(range: R2Range, totalSize: number): string {
  // R2Range can be { offset, length? } or { suffix }
  if ("suffix" in range) {
    // Suffix range: last N bytes
    const start = totalSize - range.suffix;
    return `bytes ${start}-${totalSize - 1}/${totalSize}`;
  }

  const start = range.offset ?? 0;
  const end = range.length ? start + range.length - 1 : totalSize - 1;
  return `bytes ${start}-${end}/${totalSize}`;
}
