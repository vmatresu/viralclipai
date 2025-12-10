/**
 * Clip delivery API client.
 *
 * Secure endpoints for clip playback, download, and sharing.
 * URLs are short-lived and should be used immediately.
 */

import { toast } from "sonner";

import { apiFetch } from "@/lib/apiClient";

// ============================================================================
// Types
// ============================================================================

/** Access level for shared clips */
export type ShareAccessLevel = "none" | "view_playback" | "download";

/** Summary of clip metadata */
export interface ClipSummary {
  clip_id: string;
  filename: string;
  title: string;
  duration_seconds: number;
  file_size_bytes: number;
}

/** Delivery URL response */
export interface DeliveryUrl {
  url: string;
  expires_at: string;
  expires_in_secs: number;
  content_type: string;
}

/** Playback/download URL response */
export interface PlaybackUrlResponse extends DeliveryUrl {
  clip: ClipSummary;
}

/** Request body for download URL */
export interface DownloadUrlRequest {
  filename?: string;
}

/** Request body for creating a share */
export interface CreateShareRequest {
  access_level?: ShareAccessLevel;
  expires_in_hours?: number;
  watermark_enabled?: boolean;
}

/** Share creation response */
export interface ShareResponse {
  share_url: string;
  share_slug: string;
  access_level: ShareAccessLevel;
  /** When the share expires (ISO 8601). May be undefined if field is omitted from JSON. */
  expires_at?: string | null;
  watermark_enabled: boolean;
  created_at: string;
}

// ============================================================================
// Playback URL
// ============================================================================

/**
 * Get a short-lived playback URL for a clip.
 *
 * Use this URL in a `<video>` element or window.open() for "Play in new tab".
 * URLs expire in ~15 minutes by default.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @returns Playback URL response with clip metadata
 *
 * @example
 * ```ts
 * const { url, clip } = await getPlaybackUrl("clip-123", authToken);
 * videoElement.src = url;
 * ```
 */
export function getPlaybackUrl(
  clipId: string,
  token: string
): Promise<PlaybackUrlResponse> {
  return apiFetch<PlaybackUrlResponse>(
    `/api/clips/${encodeURIComponent(clipId)}/play-url`,
    {
      method: "POST",
      token,
    }
  );
}

// ============================================================================
// Download URL
// ============================================================================

/**
 * Get a short-lived download URL for a clip.
 *
 * URLs include Content-Disposition headers to prompt browser download.
 * URLs expire in ~5 minutes by default.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @param options - Optional download options (custom filename)
 * @returns Download URL response with clip metadata
 *
 * @example
 * ```ts
 * const { url } = await getDownloadUrl("clip-123", authToken, {
 *   filename: "my-awesome-clip.mp4"
 * });
 * // Trigger download
 * const a = document.createElement('a');
 * a.href = url;
 * a.download = "my-awesome-clip.mp4";
 * a.click();
 * ```
 */
export function getDownloadUrl(
  clipId: string,
  token: string,
  options?: DownloadUrlRequest
): Promise<PlaybackUrlResponse> {
  return apiFetch<PlaybackUrlResponse>(
    `/api/clips/${encodeURIComponent(clipId)}/download-url`,
    {
      method: "POST",
      token,
      body: options ?? null,
    }
  );
}

// ============================================================================
// Thumbnail URL
// ============================================================================

/**
 * Get a short-lived thumbnail URL for a clip.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @returns Thumbnail URL
 */
export function getThumbnailUrl(clipId: string, token: string): Promise<DeliveryUrl> {
  return apiFetch<DeliveryUrl>(
    `/api/clips/${encodeURIComponent(clipId)}/thumbnail-url`,
    {
      method: "POST",
      token,
    }
  );
}

// ============================================================================
// Share Management
// ============================================================================

/**
 * Create or update a share link for a clip.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @param options - Share configuration
 * @returns Share response with public URL
 *
 * @example
 * ```ts
 * const share = await createShare("clip-123", authToken, {
 *   access_level: "view_playback",
 *   expires_in_hours: 24,
 * });
 * console.log("Share URL:", share.share_url);
 * ```
 */
export function createShare(
  clipId: string,
  token: string,
  options?: CreateShareRequest
): Promise<ShareResponse> {
  return apiFetch<ShareResponse>(`/api/clips/${encodeURIComponent(clipId)}/share`, {
    method: "POST",
    token,
    body: options ?? { access_level: "view_playback" },
  });
}

/**
 * Revoke a share link for a clip.
 *
 * After revocation, the share URL will return 410 Gone.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 */
export function revokeShare(clipId: string, token: string): Promise<void> {
  return apiFetch<void>(`/api/clips/${encodeURIComponent(clipId)}/share`, {
    method: "DELETE",
    token,
  });
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Check if a delivery URL is still valid (not expired).
 *
 * @param url - Delivery URL response
 * @param bufferSeconds - Buffer time before expiry (default: 60s)
 * @returns true if URL is still valid
 */
export function isUrlValid(
  url: DeliveryUrl | PlaybackUrlResponse,
  bufferSeconds = 60
): boolean {
  const expiresAt = new Date(url.expires_at);
  const now = new Date();
  const bufferMs = bufferSeconds * 1000;
  return expiresAt.getTime() > now.getTime() + bufferMs;
}

/**
 * Open a clip in a new tab for playback.
 *
 * NOTE: This function uses browser APIs (window) and must only be called
 * from client components. It will throw in SSR/edge contexts.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 */
export async function playInNewTab(clipId: string, token: string): Promise<void> {
  if (typeof window === "undefined") {
    throw new Error("playInNewTab can only be called in browser context");
  }
  const { url } = await getPlaybackUrl(clipId, token);
  window.open(url, "_blank");
}

/**
 * Trigger a clip download.
 *
 * NOTE: This function uses browser APIs (document) and must only be called
 * from client components. It will throw in SSR/edge contexts.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @param filename - Optional custom filename
 */
export async function downloadClip(
  clipId: string,
  token: string,
  filename?: string
): Promise<void> {
  if (typeof document === "undefined") {
    throw new Error("downloadClip can only be called in browser context");
  }
  const { url, clip } = await getDownloadUrl(clipId, token, { filename });
  const downloadName = filename ?? clip.filename;

  // Create a temporary anchor to trigger download
  const a = document.createElement("a");
  a.href = url;
  a.download = downloadName;
  a.style.display = "none";
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

/**
 * Copy a share URL to clipboard with toast feedback.
 *
 * Creates a share if needed and copies the URL. Shows sonner toast notifications
 * for success and failure states.
 *
 * NOTE: This function uses browser APIs (navigator.clipboard) and must only
 * be called from client components. It will silently fail in SSR/edge contexts.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @param options - Share configuration
 * @returns The share URL on success, or undefined on failure
 */
export async function copyShareUrl(
  clipId: string,
  token: string,
  options?: CreateShareRequest
): Promise<string | undefined> {
  // Guard against SSR / No Clipboard
  if (
    typeof window === "undefined" ||
    typeof navigator === "undefined" ||
    !navigator.clipboard
  ) {
    toast.error("Clipboard not available");
    return undefined;
  }

  const toastId = toast.loading("Generating share link...");

  try {
    // Create or get share
    const share = await createShare(clipId, token, options);

    // Copy to clipboard
    await navigator.clipboard.writeText(share.share_url);

    toast.success("Share link copied!", {
      id: toastId,
      description: "Anyone with this link can view the clip.",
    });

    return share.share_url;
  } catch (err) {
    console.error("[copyShareUrl] Failed:", err);
    toast.error("Failed to copy link", {
      id: toastId,
      description: err instanceof Error ? err.message : "Unknown error",
    });
    return undefined;
  }
}

/**
 * Copy a share URL to clipboard (throwing version).
 *
 * Like copyShareUrl but throws on error instead of showing toast.
 * Use this when you want to handle errors yourself.
 *
 * @param clipId - The clip ID
 * @param token - Auth token
 * @param options - Share configuration
 * @returns The share URL on success
 * @throws Error if clipboard is unavailable or access is denied
 */
export async function copyShareUrlOrThrow(
  clipId: string,
  token: string,
  options?: CreateShareRequest
): Promise<string> {
  if (typeof navigator === "undefined" || !navigator.clipboard) {
    throw new Error("Clipboard API is not available in this context");
  }

  const share = await createShare(clipId, token, options);

  try {
    await navigator.clipboard.writeText(share.share_url);
  } catch (err) {
    throw new Error(
      `Failed to copy to clipboard: ${err instanceof Error ? err.message : "Permission denied"}`
    );
  }

  return share.share_url;
}
