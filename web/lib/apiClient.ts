import { sanitizeUrl } from "@/lib/security/validation";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL ?? "";

export interface ApiRequestOptions {
  method?: string;
  token?: string | null;
  body?: unknown;
}

export function getUserVideos<T = unknown>(
  token: string,
  options?: {
    limit?: number;
    pageToken?: string | null;
    sortField?: string;
    sortDirection?: "asc" | "desc";
  }
): Promise<{ videos: T[]; next_page_token?: string | null }> {
  const limit = options?.limit;
  const pageToken = options?.pageToken;
  const sortField = options?.sortField;
  const sortDirection = options?.sortDirection;

  const params = new URLSearchParams();
  if (typeof limit === "number") {
    params.set("limit", String(limit));
  }
  if (pageToken) {
    params.set("page_token", pageToken);
  }
  if (sortField) {
    params.set("sort_field", sortField);
  }
  if (sortDirection) {
    params.set("sort_direction", sortDirection);
  }

  const path = params.toString()
    ? `/api/user/videos?${params.toString()}`
    : "/api/user/videos";

  return apiFetch(path, { token });
}

export function getProcessingStatuses(
  token: string,
  videoIds: string[]
): Promise<{
  videos: Array<{
    video_id: string;
    status?: "processing" | "analyzed" | "completed" | "failed";
    clips_count?: number;
    updated_at?: string;
  }>;
}> {
  const ids = videoIds.join(",");
  return apiFetch(`/api/user/videos/processing-status?ids=${encodeURIComponent(ids)}`, {
    token,
  });
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
      // Security: prevent caching of sensitive/user-specific data
      // Always use no-store to ensure fresh data (settings, plans, usage counts)
      cache: "no-store",
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
 * Update clip title
 */
export function updateClipTitle(
  videoId: string,
  clipId: string,
  title: string,
  token: string
): Promise<{
  success: boolean;
  clip_id: string;
  new_title: string;
}> {
  return apiFetch<{
    success: boolean;
    clip_id: string;
    new_title: string;
  }>(
    `/api/videos/${encodeURIComponent(videoId)}/clips/${encodeURIComponent(clipId)}/title`,
    {
      method: "PATCH",
      token,
      body: { title },
    }
  );
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
 * Get video details including clips
 */
export function getVideoDetails(
  videoId: string,
  token: string
): Promise<{
  clips: Array<{
    clip_id: string;
    name: string;
    title: string;
    description: string;
    url: string;
    direct_url?: string | null;
    thumbnail?: string | null;
    size: string;
    style?: string;
    completed_at?: string | null;
    updated_at?: string | null;
  }>;
  custom_prompt?: string;
  video_title?: string;
  video_url?: string;
}> {
  return apiFetch(`/api/videos/${encodeURIComponent(videoId)}`, {
    method: "GET",
    token,
  });
}

/**
 * Get existing scene/style combinations for a video
 */
export function getVideoSceneStyles(
  videoId: string,
  token: string
): Promise<{
  video_id: string;
  scene_styles: Array<{
    scene_id: number;
    scene_title?: string;
    styles: string[];
  }>;
}> {
  return apiFetch(`/api/videos/${encodeURIComponent(videoId)}/scene-styles`, {
    method: "GET",
    token,
  });
}

/**
 * Reprocess scenes for an existing video via REST API.
 * This replaces WebSocket-based reprocessing.
 */
export interface ReprocessRequest {
  scene_ids: number[];
  styles: string[];
  overwrite?: boolean;
  enable_object_detection?: boolean;
  top_scenes_compilation?: boolean;
  cut_silent_parts?: boolean;
  streamer_split_params?: {
    position_x: string;
    position_y: string;
    zoom: number;
  };
}

export interface ReprocessResponse {
  job_id: string;
  video_id: string;
  status: "queued" | "processing";
  total_clips: number;
  message?: string;
}

export function reprocessScenes(
  videoId: string,
  request: ReprocessRequest,
  token: string
): Promise<ReprocessResponse> {
  return apiFetch<ReprocessResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/reprocess`,
    {
      method: "POST",
      token,
      body: request,
    }
  );
}

// ============================================================================
// Credit History Types and Functions
// ============================================================================

export type CreditOperationType =
  | "analysis"
  | "scene_processing"
  | "reprocessing"
  | "silent_remover"
  | "object_detection"
  | "scene_originals"
  | "generate_more_scenes"
  | "admin_adjustment";

// ============================================================================
// Highlight Management Types and Functions
// ============================================================================

export interface HighlightInfo {
  id: number;
  title: string;
  start: string;
  end: string;
  duration: number;
  reason?: string;
  description?: string;
  hook_category?: string;
}

export interface UpdateSceneTimestampsResponse {
  success: boolean;
  video_id: string;
  scene_id: number;
  start: string;
  end: string;
  duration: number;
}

/**
 * Update a scene's timestamps (FREE)
 */
export function updateSceneTimestamps(
  videoId: string,
  sceneId: number,
  start: string,
  end: string,
  token: string
): Promise<UpdateSceneTimestampsResponse> {
  return apiFetch<UpdateSceneTimestampsResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/highlights/${sceneId}`,
    {
      method: "PATCH",
      token,
      body: { start, end },
    }
  );
}

export interface AddSceneRequest {
  title: string;
  reason: string;
  start: string;
  end: string;
  description?: string;
  hook_category?: string;
}

export interface AddSceneResponse {
  success: boolean;
  video_id: string;
  scene: HighlightInfo;
}

/**
 * Add a single scene (FREE)
 */
export function addScene(
  videoId: string,
  scene: AddSceneRequest,
  token: string
): Promise<AddSceneResponse> {
  return apiFetch<AddSceneResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/highlights`,
    {
      method: "POST",
      token,
      body: scene,
    }
  );
}

export interface BulkSceneEntry {
  title: string;
  reason: string;
  start: string;
  end: string;
  description?: string;
  hook_category?: string;
}

export interface SceneValidationError {
  index: number;
  error: string;
}

export interface BulkAddScenesResponse {
  success: boolean;
  video_id: string;
  added_count: number;
  scenes: HighlightInfo[];
  errors: SceneValidationError[];
}

/**
 * Bulk add scenes (up to 30, FREE)
 */
export function bulkAddScenes(
  videoId: string,
  scenes: BulkSceneEntry[],
  token: string
): Promise<BulkAddScenesResponse> {
  return apiFetch<BulkAddScenesResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/highlights/bulk`,
    {
      method: "POST",
      token,
      body: { scenes },
    }
  );
}

export interface GenerateMoreScenesResponse {
  success: boolean;
  video_id: string;
  generated_count: number;
  scenes: HighlightInfo[];
  credits_charged: number;
}

/**
 * Generate more scenes using AI (costs 3 credits)
 */
export function generateMoreScenes(
  videoId: string,
  count: number,
  idempotencyKey: string,
  token: string
): Promise<GenerateMoreScenesResponse> {
  return apiFetch<GenerateMoreScenesResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/highlights/generate-more`,
    {
      method: "POST",
      token,
      body: { count, idempotency_key: idempotencyKey },
    }
  );
}

export interface DeleteSceneResponse {
  success: boolean;
  video_id: string;
  scene_id: number;
}

/**
 * Delete a scene from highlights
 */
export function deleteHighlightScene(
  videoId: string,
  sceneId: number,
  token: string
): Promise<DeleteSceneResponse> {
  return apiFetch<DeleteSceneResponse>(
    `/api/videos/${encodeURIComponent(videoId)}/highlights/${sceneId}`,
    {
      method: "DELETE",
      token,
    }
  );
}

export interface CreditTransaction {
  id: string;
  timestamp: string;
  operation_type: CreditOperationType;
  credits_amount: number;
  description: string;
  balance_after: number;
  video_id?: string;
  draft_id?: string;
  metadata?: Record<string, string>;
}

export interface MonthSummary {
  month: string;
  total_used: number;
  monthly_limit: number;
  remaining: number;
  by_operation: Record<string, number>;
}

export interface CreditHistoryResponse {
  transactions: CreditTransaction[];
  next_page_token?: string;
  summary: MonthSummary;
}

export interface CreditHistoryOptions {
  /** Maximum number of transactions to return (clamped to 1..100 server-side) */
  limit?: number;
  /** Cursor timestamp for pagination (ISO8601 format from previous response) */
  cursor?: string;
  /** Filter by operation type */
  operationType?: CreditOperationType;
}

/**
 * Get credit usage history for the authenticated user.
 *
 * Uses cursor-based pagination with server-side ordering (newest first).
 * The `next_page_token` in the response is now a timestamp cursor.
 */
export function getCreditHistory(
  token: string,
  options?: CreditHistoryOptions
): Promise<CreditHistoryResponse> {
  const params = new URLSearchParams();
  if (options?.limit) {
    params.set("limit", String(options.limit));
  }
  if (options?.cursor) {
    params.set("cursor", options.cursor);
  }
  if (options?.operationType) {
    params.set("operation_type", options.operationType);
  }

  const path = params.toString()
    ? `/api/credits/history?${params.toString()}`
    : "/api/credits/history";

  return apiFetch<CreditHistoryResponse>(path, { token });
}
