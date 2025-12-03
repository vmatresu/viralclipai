/**
 * Custom React Hook for Clips Cache
 *
 * Provides a convenient hook interface for cache operations
 * with proper React lifecycle management.
 */

import { useCallback } from "react";

import {
  getCachedClips,
  setCachedClips,
  invalidateClipsCache,
  type CachedClipsData,
} from "./index";

/**
 * Hook for managing clips cache
 *
 * @returns Object with cache operations
 */
export function useClipsCache() {
  const get = useCallback((videoId: string) => {
    return getCachedClips(videoId);
  }, []);

  const set = useCallback(
    (videoId: string, data: Omit<CachedClipsData, "_metadata">) => {
      return setCachedClips(videoId, data);
    },
    []
  );

  const invalidate = useCallback((videoId: string) => {
    return invalidateClipsCache(videoId);
  }, []);

  return {
    get,
    set,
    invalidate,
  };
}
