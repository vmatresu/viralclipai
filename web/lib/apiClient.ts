import { sanitizeUrl } from "@/lib/security/validation";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL ?? "";

export interface ApiRequestOptions {
  method?: string;
  token?: string | null;
  body?: unknown;
}

/**
 * Sanitizes error messages to prevent information leakage
 */
function sanitizeError(error: unknown, status: number): Error {
  // Don't expose internal error details to clients
  // Only return safe, generic error messages
  if (status >= 500) {
    return new Error("An internal server error occurred. Please try again later.");
  }
  if (status === 401) {
    return new Error("Authentication required. Please sign in.");
  }
  if (status === 403) {
    return new Error("You don't have permission to perform this action.");
  }
  if (status === 404) {
    return new Error("The requested resource was not found.");
  }
  if (status === 429) {
    return new Error("Too many requests. Please try again later.");
  }

  // For client errors (4xx), we can be slightly more specific
  // but still avoid exposing sensitive details
  const message = error instanceof Error ? error.message : "Request failed";
  // Limit error message length to prevent DoS
  const safeMessage = message.length > 200 ? message.substring(0, 200) : message;
  return new Error(safeMessage);
}

export async function apiFetch<T = unknown>(
  path: string,
  options: ApiRequestOptions = {}
): Promise<T> {
  const { method = "GET", token, body } = options;

  // Validate and sanitize path
  if (!path || typeof path !== "string") {
    throw new Error("Invalid API path");
  }

  // Prevent path traversal attacks
  if (path.includes("..") || path.includes("//")) {
    throw new Error("Invalid API path");
  }

  // Ensure path starts with /
  const sanitizedPath = path.startsWith("/") ? path : `/${path}`;

  // Build URL safely
  let url: string;
  if (API_BASE_URL) {
    const sanitizedBase = sanitizeUrl(API_BASE_URL);
    if (!sanitizedBase) {
      throw new Error("Invalid API base URL configuration");
    }
    // Remove trailing slash from base if present to prevent double slashes
    const cleanBase = sanitizedBase.endsWith("/")
      ? sanitizedBase.slice(0, -1)
      : sanitizedBase;
    url = `${cleanBase}${sanitizedPath}`;
  } else {
    // Relative URL - ensure it's safe
    url = sanitizedPath;
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) {
    // Validate token is not empty
    if (typeof token !== "string" || token.trim() === "") {
      throw new Error("Invalid authentication token");
    }
    headers["Authorization"] = `Bearer ${token}`;
  }

  try {
    const res = await fetch(url, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
      // Security: don't send credentials unless explicitly needed
      credentials: "same-origin",
      // Security: prevent caching of sensitive data
      cache: method === "GET" ? "default" : "no-store",
    });

    if (!res.ok) {
      let errorText: string;
      try {
        errorText = await res.text();
        // Limit error text length
        if (errorText.length > 1000) {
          errorText = errorText.substring(0, 1000);
        }
      } catch {
        errorText = "";
      }
      throw sanitizeError(errorText || new Error("Request failed"), res.status);
    }

    if (res.status === 204) {
      return undefined as unknown as T;
    }

    return (await res.json()) as T;
  } catch (error) {
    // Re-throw sanitized errors
    if (error instanceof Error) {
      throw error;
    }
    throw new Error("An unexpected error occurred");
  }
}

/**
 * Delete a single video
 */
export function deleteVideo(
  videoId: string,
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  message?: string;
  files_deleted?: number;
}> {
  return apiFetch<{
    success: boolean;
    video_id: string;
    message?: string;
    files_deleted?: number;
  }>(`/api/videos/${encodeURIComponent(videoId)}`, {
    method: "DELETE",
    token,
  });
}

/**
 * Delete multiple videos
 */
export function bulkDeleteVideos(
  videoIds: string[],
  token: string
): Promise<{
  success: boolean;
  deleted_count: number;
  failed_count: number;
  results: Record<string, { success: boolean; error?: string; files_deleted?: number }>;
}> {
  return apiFetch<{
    success: boolean;
    deleted_count: number;
    failed_count: number;
    results: Record<
      string,
      { success: boolean; error?: string; files_deleted?: number }
    >;
  }>("/api/videos", {
    method: "DELETE",
    token,
    body: { video_ids: videoIds },
  });
}

/**
 * Delete a single clip from a video
 */
export function deleteClip(
  videoId: string,
  clipName: string,
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  clip_name: string;
  message?: string;
  files_deleted?: number;
}> {
  return apiFetch<{
    success: boolean;
    video_id: string;
    clip_name: string;
    message?: string;
    files_deleted?: number;
  }>(
    `/api/videos/${encodeURIComponent(videoId)}/clips/${encodeURIComponent(clipName)}`,
    {
      method: "DELETE",
      token,
    }
  );
}

/**
 * Delete multiple clips from a video
 */
export function bulkDeleteClips(
  videoId: string,
  clipNames: string[],
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  deleted_count: number;
  failed_count: number;
  results: Record<string, { success: boolean; error?: string; files_deleted?: number }>;
}> {
  return apiFetch<{
    success: boolean;
    video_id: string;
    deleted_count: number;
    failed_count: number;
    results: Record<
      string,
      { success: boolean; error?: string; files_deleted?: number }
    >;
  }>(`/api/videos/${encodeURIComponent(videoId)}/clips`, {
    method: "DELETE",
    token,
    body: { clip_names: clipNames },
  });
}

/**
 * Delete all clips from a video
 */
export function deleteAllClips(
  videoId: string,
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  deleted_count: number;
  failed_count: number;
  results: Record<string, { success: boolean; error?: string; files_deleted?: number }>;
  message?: string;
}> {
  return apiFetch<{
    success: boolean;
    video_id: string;
    deleted_count: number;
    failed_count: number;
    results: Record<
      string,
      { success: boolean; error?: string; files_deleted?: number }
    >;
    message?: string;
  }>(`/api/videos/${encodeURIComponent(videoId)}/clips/all`, {
    method: "DELETE",
    token,
  });
}
export function updateVideoTitle(
  videoId: string,
  title: string,
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  title: string;
  message?: string;
}> {
  return apiFetch<{
    success: boolean;
    video_id: string;
    title: string;
    message?: string;
  }>(`/api/videos/${encodeURIComponent(videoId)}/title`, {
    method: "PATCH",
    token,
    body: { title },
  });
}

/**
 * Get highlights for a video
 */
export function getVideoHighlights(
  videoId: string,
  token: string
): Promise<{
  video_id: string;
  video_url?: string;
  video_title?: string;
  highlights: Array<{
    id: number;
    title: string;
    start: string;
    end: string;
    duration: number;
    hook_category?: string;
    reason?: string;
    description?: string;
  }>;
}> {
  return apiFetch(`/api/videos/${encodeURIComponent(videoId)}/highlights`, {
    method: "GET",
    token,
  });
}

/**
 * Reprocess scenes from a video
 */
export function reprocessScenes(
  videoId: string,
  sceneIds: number[],
  styles: string[],
  token: string
): Promise<{
  success: boolean;
  video_id: string;
  message: string;
  job_id?: string;
}> {
  return apiFetch(`/api/videos/${encodeURIComponent(videoId)}/reprocess`, {
    method: "POST",
    token,
    body: { scene_ids: sceneIds, styles },
  });
}
