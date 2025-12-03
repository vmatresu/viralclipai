/**
 * Cache Module
 * 
 * Centralized exports for the caching system.
 * Provides a singleton cache manager instance.
 */

import { CacheManager } from "./CacheManager";
import { LocalStorageAdapter } from "./storage/localStorageAdapter";
import type { CacheConfig, CachedClipsData } from "./types";

// Create singleton cache manager instance
const storageAdapter = new LocalStorageAdapter("viralclipai_clips_");
const cacheManager = new CacheManager(storageAdapter, {
  ttl: 24 * 60 * 60 * 1000, // 24 hours
  maxEntries: 100,
  maxSizeBytes: 10 * 1024 * 1024, // 10MB
  version: 1,
  enableLRU: true,
  cleanupInterval: 60 * 60 * 1000, // 1 hour
});

/**
 * Get cached clips data for a video ID
 */
export async function getCachedClips(
  videoId: string
): Promise<CachedClipsData | null> {
  return cacheManager.get(videoId);
}

/**
 * Cache clips data for a video ID
 */
export async function setCachedClips(
  videoId: string,
  data: Omit<CachedClipsData, "_metadata">
): Promise<void> {
  return cacheManager.set(videoId, data);
}

/**
 * Invalidate cache for a specific video ID
 */
export async function invalidateClipsCache(videoId: string): Promise<void> {
  return cacheManager.invalidate(videoId);
}

/**
 * Invalidate cache for multiple video IDs
 */
export async function invalidateClipsCacheMany(
  videoIds: string[]
): Promise<void> {
  return cacheManager.invalidateMany(videoIds);
}

/**
 * Clear all clips cache
 */
export async function clearAllClipsCache(): Promise<void> {
  return cacheManager.clear();
}

/**
 * Get cache statistics
 */
export function getCacheStats() {
  return cacheManager.getStats();
}

/**
 * Subscribe to cache events (for observability/debugging)
 */
export function onCacheEvent(
  eventType: "hit" | "miss" | "set" | "evict" | "invalidate" | "error" | "cleanup",
  listener: (event: { type: string; key?: string; timestamp: number; metadata?: Record<string, unknown> }) => void
): () => void {
  return cacheManager.on(eventType, listener);
}

// Export types for external use
export type { CacheConfig, CacheStats, CachedClipsData } from "./types";

