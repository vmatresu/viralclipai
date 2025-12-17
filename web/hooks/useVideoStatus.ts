"use client";

/**
 * Simple video status fetching hook with exponential cache
 *
 * Replaces WebSocket-based real-time updates with simple fetch-on-demand.
 * Uses localStorage cache with exponential backoff to avoid hammering Firebase.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";

// ============================================================================
// Types
// ============================================================================

export interface ProcessingProgress {
  total_scenes: number;
  completed_scenes: number;
  total_clips: number;
  completed_clips: number;
  failed_clips: number;
  current_scene_id?: number;
  current_scene_title?: string;
  started_at: string;
  updated_at: string;
  error_message?: string;
}

export interface VideoStatus {
  id: string;
  title?: string;
  /**
   * Video status:
   * - pending: Initial state, not yet started
   * - processing: Currently being processed (analysis or clip rendering)
   * - analyzed: AI analysis complete, scenes available for selection (no clips rendered)
   * - completed: Clips have been rendered successfully
   * - failed: Processing failed
   */
  status: "pending" | "processing" | "analyzed" | "completed" | "failed";
  clip_count: number;
  created_at: string;
  updated_at: string;
  processing_progress?: ProcessingProgress;
}

interface CachedStatus {
  data: VideoStatus;
  timestamp: number;
  fetchCount: number;
}

// ============================================================================
// Cache Configuration
// ============================================================================

const CACHE_KEY_PREFIX = "vclip_video_status_";
const BASE_TTL_MS = 30_000; // 30 seconds base TTL
const MAX_TTL_MS = 5 * 60_000; // 5 minutes max TTL
const BACKOFF_MULTIPLIER = 1.5;

// ============================================================================
// Cache Helpers
// ============================================================================

function getCacheKey(videoId: string): string {
  return `${CACHE_KEY_PREFIX}${videoId}`;
}

function getCached(videoId: string): CachedStatus | null {
  try {
    const key = getCacheKey(videoId);
    const stored = localStorage.getItem(key);
    if (!stored) return null;
    return JSON.parse(stored) as CachedStatus;
  } catch {
    return null;
  }
}

function setCached(videoId: string, data: VideoStatus, fetchCount: number): void {
  try {
    const key = getCacheKey(videoId);
    const cached: CachedStatus = {
      data,
      timestamp: Date.now(),
      fetchCount,
    };
    localStorage.setItem(key, JSON.stringify(cached));
  } catch (e) {
    frontendLogger.error("Failed to cache video status:", e);
  }
}

function clearCache(videoId: string): void {
  try {
    const key = getCacheKey(videoId);
    localStorage.removeItem(key);
  } catch {
    // Ignore
  }
}

function calculateTtl(fetchCount: number): number {
  const ttl = BASE_TTL_MS * Math.pow(BACKOFF_MULTIPLIER, fetchCount);
  return Math.min(ttl, MAX_TTL_MS);
}

// ============================================================================
// API Fetching
// ============================================================================

async function fetchVideoStatus(
  videoId: string,
  token: string
): Promise<VideoStatus | null> {
  try {
    const data = await apiFetch<VideoStatus>(`/api/videos/${videoId}`, { token });
    return data;
  } catch (error) {
    // Handle 404 as null (video not found)
    if (error instanceof Error && error.message.includes("not found")) {
      return null;
    }
    throw error;
  }
}

// ============================================================================
// Hook
// ============================================================================

export interface UseVideoStatusOptions {
  /** Skip initial fetch (useful if you want to control when to fetch) */
  skipInitialFetch?: boolean;
  /** Force fresh fetch, ignoring cache */
  forceFresh?: boolean;
}

export interface UseVideoStatusReturn {
  /** Current video status */
  status: VideoStatus | null;
  /** Loading state */
  loading: boolean;
  /** Error state */
  error: string | null;
  /** Manually refresh status (respects cache unless force=true) */
  refresh: (force?: boolean) => Promise<void>;
  /** Clear cache for this video */
  clearCache: () => void;
}

export function useVideoStatus(
  videoId: string | null,
  options: UseVideoStatusOptions = {}
): UseVideoStatusReturn {
  const { skipInitialFetch = false, forceFresh = false } = options;
  const { getIdToken } = useAuth();

  const [status, setStatus] = useState<VideoStatus | null>(null);
  const [loading, setLoading] = useState(!skipInitialFetch);
  const [error, setError] = useState<string | null>(null);

  const fetchCountRef = useRef(0);

  const doFetch = useCallback(
    async (force = false) => {
      if (!videoId) {
        setStatus(null);
        setLoading(false);
        return;
      }

      // Check cache first (unless forced)
      if (!force && !forceFresh) {
        const cached = getCached(videoId);
        if (cached) {
          const ttl = calculateTtl(cached.fetchCount);
          const age = Date.now() - cached.timestamp;

          // IMPORTANT: Never trust cache if status is "processing" - it's transient and
          // likely to have changed. Always fetch fresh data to ensure we show current state.
          const isProcessingStatus = cached.data.status === "processing";

          if (age < ttl && !isProcessingStatus) {
            // Cache is still valid and not in processing state
            setStatus(cached.data);
            setLoading(false);
            fetchCountRef.current = cached.fetchCount;
            return;
          }
        }
      }

      // Fetch fresh data
      setLoading(true);
      setError(null);

      try {
        const token = await getIdToken();
        if (!token) {
          setError("Not authenticated");
          setLoading(false);
          return;
        }

        const data = await fetchVideoStatus(videoId, token);
        if (data) {
          fetchCountRef.current++;
          setCached(videoId, data, fetchCountRef.current);
          setStatus(data);
        } else {
          setStatus(null);
          setError("Video not found");
        }
      } catch (e) {
        const message = e instanceof Error ? e.message : "Failed to fetch status";
        setError(message);
        frontendLogger.error("Failed to fetch video status:", e);
      } finally {
        setLoading(false);
      }
    },
    [videoId, getIdToken, forceFresh]
  );

  // Initial fetch
  useEffect(() => {
    if (!skipInitialFetch && videoId) {
      void doFetch();
    }
  }, [videoId, skipInitialFetch, doFetch]);

  const refresh = useCallback(
    async (force = false) => {
      await doFetch(force);
    },
    [doFetch]
  );

  const clearCacheCallback = useCallback(() => {
    if (videoId) {
      clearCache(videoId);
      fetchCountRef.current = 0;
    }
  }, [videoId]);

  return {
    status,
    loading,
    error,
    refresh,
    clearCache: clearCacheCallback,
  };
}

// ============================================================================
// Batch Hook for Multiple Videos
// ============================================================================

export interface UseVideosStatusReturn {
  /** Map of video ID to status */
  statuses: Map<string, VideoStatus>;
  /** Loading state */
  loading: boolean;
  /** Error state */
  error: string | null;
  /** Refresh all statuses */
  refresh: (force?: boolean) => Promise<void>;
}

export function useVideosStatus(
  videoIds: string[],
  options: UseVideoStatusOptions = {}
): UseVideosStatusReturn {
  const { skipInitialFetch = false } = options;
  const { getIdToken } = useAuth();

  const [statuses, setStatuses] = useState<Map<string, VideoStatus>>(new Map());
  const [loading, setLoading] = useState(!skipInitialFetch);
  const [error, setError] = useState<string | null>(null);

  const doFetch = useCallback(
    async (force = false) => {
      if (videoIds.length === 0) {
        setStatuses(new Map());
        setLoading(false);
        return;
      }

      setLoading(true);
      setError(null);

      try {
        const token = await getIdToken();
        if (!token) {
          setError("Not authenticated");
          setLoading(false);
          return;
        }

        const results = new Map<string, VideoStatus>();

        // Fetch in parallel with concurrency limit
        const BATCH_SIZE = 5;
        const batches: string[][] = [];
        for (let i = 0; i < videoIds.length; i += BATCH_SIZE) {
          batches.push(videoIds.slice(i, i + BATCH_SIZE));
        }

        await batches.reduce<Promise<void>>(async (prev, batch) => {
          await prev;

          const promises = batch.map(async (videoId) => {
            // Check cache first (unless forced)
            if (!force) {
              const cached = getCached(videoId);
              if (cached) {
                const ttl = calculateTtl(cached.fetchCount);
                const age = Date.now() - cached.timestamp;
                if (age < ttl) {
                  return { videoId, data: cached.data, fromCache: true };
                }
              }
            }

            // Fetch fresh
            const data = await fetchVideoStatus(videoId, token);
            if (data) {
              // Properly track fetchCount per video
              const existingCache = getCached(videoId);
              const newFetchCount = (existingCache?.fetchCount ?? 0) + 1;
              setCached(videoId, data, newFetchCount);
            }
            return { videoId, data, fromCache: false };
          });

          const batchResults = await Promise.allSettled(promises);
          for (const result of batchResults) {
            if (result.status === "fulfilled" && result.value.data) {
              results.set(result.value.videoId, result.value.data);
            }
          }
        }, Promise.resolve());

        setStatuses(results);
      } catch (e) {
        const message = e instanceof Error ? e.message : "Failed to fetch statuses";
        setError(message);
        frontendLogger.error("Failed to fetch video statuses:", e);
      } finally {
        setLoading(false);
      }
    },
    [videoIds, getIdToken]
  );

  // Initial fetch - depend on stable key for IDs, not just length
  const videoIdsKey = videoIds.join(",");
  useEffect(() => {
    if (!skipInitialFetch && videoIdsKey) {
      void doFetch();
    }
  }, [videoIdsKey, skipInitialFetch, doFetch]);

  const refresh = useCallback(
    async (force = false) => {
      await doFetch(force);
    },
    [doFetch]
  );

  return {
    statuses,
    loading,
    error,
    refresh,
  };
}

// ============================================================================
// Utility: Calculate progress percentage
// ============================================================================

export function calculateProgressPercentage(progress?: ProcessingProgress): number {
  if (!progress || progress.total_clips === 0) {
    return 0;
  }
  return Math.round((progress.completed_clips / progress.total_clips) * 100);
}
