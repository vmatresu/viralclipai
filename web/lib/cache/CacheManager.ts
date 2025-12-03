/**
 * Cache Manager
 *
 * Production-ready cache manager with LRU eviction, versioning,
 * and comprehensive error handling.
 *
 * Implements SOLID principles:
 * - Single Responsibility: Manages cache operations only
 * - Open/Closed: Extensible via storage adapters
 * - Dependency Inversion: Depends on IStorageAdapter abstraction
 */

import { frontendLogger } from "@/lib/logger";

import type {
  CacheConfig,
  CacheEvent,
  CacheStats,
  CachedClipsData,
  IStorageAdapter,
} from "./types";

/**
 * Default cache configuration
 */
const DEFAULT_CONFIG: CacheConfig = {
  ttl: 24 * 60 * 60 * 1000, // 24 hours
  maxEntries: 100,
  maxSizeBytes: 10 * 1024 * 1024, // 10MB
  version: 1,
  enableLRU: true,
  cleanupInterval: 60 * 60 * 1000, // 1 hour
};

/**
 * Cache Manager class
 */
export class CacheManager {
  private readonly config: CacheConfig;
  private readonly storage: IStorageAdapter;
  private readonly logger = frontendLogger;
  private stats: CacheStats;
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;
  private readonly eventListeners: Map<
    CacheEvent["type"],
    Set<(event: CacheEvent) => void>
  > = new Map();

  constructor(storage: IStorageAdapter, config: Partial<CacheConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.storage = storage;
    this.stats = {
      totalEntries: 0,
      totalSize: 0,
      hits: 0,
      misses: 0,
      evictions: 0,
      errors: 0,
    };

    // Start periodic cleanup
    this.startCleanupTimer();

    // Initialize stats
    this.refreshStats().catch((error) => {
      this.logger.warn("Failed to initialize cache stats", { error });
    });
  }

  /**
   * Get cached clips data
   */
  async get(videoId: string): Promise<CachedClipsData | null> {
    try {
      const key = this.getCacheKey(videoId);
      const entry = await this.storage.get<CachedClipsData>(key);

      if (!entry) {
        this.recordMiss();
        this.emitEvent("miss", key);
        return null;
      }

      // Check version compatibility
      if (entry._metadata.version !== this.config.version) {
        this.logger.debug("Cache version mismatch, invalidating", {
          cachedVersion: entry._metadata.version,
          currentVersion: this.config.version,
        });
        await this.invalidate(videoId);
        this.recordMiss();
        this.emitEvent("miss", key);
        return null;
      }

      // Check TTL
      const age = Date.now() - entry._metadata.cachedAt;
      if (age >= this.config.ttl) {
        await this.invalidate(videoId);
        this.recordMiss();
        this.emitEvent("miss", key);
        return null;
      }

      // Update access metadata
      entry._metadata.lastAccessedAt = Date.now();
      entry._metadata.accessCount += 1;

      // Persist updated metadata
      await this.storage.set(key, entry).catch((error) => {
        this.logger.warn("Failed to update cache metadata", { key, error });
      });

      this.recordHit();
      this.emitEvent("hit", key);
      return entry;
    } catch (error) {
      this.recordError();
      this.logger.error("Failed to get cache entry", { videoId, error });
      this.emitEvent("error", undefined, { error: String(error) });
      return null;
    }
  }

  /**
   * Set cached clips data
   */
  async set(videoId: string, data: Omit<CachedClipsData, "_metadata">): Promise<void> {
    const key = this.getCacheKey(videoId);

    // Create cache entry with metadata
    const entry: CachedClipsData = {
      ...data,
      _metadata: {
        cachedAt: Date.now(),
        lastAccessedAt: Date.now(),
        accessCount: 0,
        version: this.config.version,
      },
    };

    // Check size before storing
    const size = this.estimateEntrySize(entry);

    try {
      // Evict if necessary
      await this.evictIfNeeded(size);

      // Store entry
      await this.storage.set(key, entry);

      await this.refreshStats();
      this.emitEvent("set", key, { size });
    } catch (error) {
      this.recordError();

      // If quota exceeded, try evicting and retry once
      if (error instanceof Error && error.message === "Storage quota exceeded") {
        this.logger.warn("Storage quota exceeded, attempting eviction", { videoId });
        try {
          await this.evictOldest(1);
          await this.storage.set(key, entry);
          await this.refreshStats();
          this.emitEvent("set", key, { size, retried: true });
        } catch (retryError) {
          this.logger.error("Failed to set cache entry after eviction", {
            videoId,
            error: retryError,
          });
          throw retryError;
        }
      } else {
        this.logger.error("Failed to set cache entry", { videoId, error });
        throw error;
      }
    }
  }

  /**
   * Invalidate cache for a video ID
   */
  async invalidate(videoId: string): Promise<void> {
    try {
      const key = this.getCacheKey(videoId);
      await this.storage.remove(key);
      await this.refreshStats();
      this.emitEvent("invalidate", key);
    } catch (error) {
      this.recordError();
      this.logger.warn("Failed to invalidate cache entry", { videoId, error });
    }
  }

  /**
   * Invalidate multiple cache entries
   */
  async invalidateMany(videoIds: string[]): Promise<void> {
    await Promise.allSettled(videoIds.map((videoId) => this.invalidate(videoId)));
  }

  /**
   * Clear all cache entries
   */
  async clear(): Promise<void> {
    try {
      await this.storage.clear();
      await this.refreshStats();
      this.emitEvent("cleanup");
    } catch (error) {
      this.recordError();
      this.logger.error("Failed to clear cache", { error });
      throw error;
    }
  }

  /**
   * Get cache statistics
   */
  getStats(): CacheStats {
    return { ...this.stats };
  }

  /**
   * Subscribe to cache events
   */
  on(eventType: CacheEvent["type"], listener: (event: CacheEvent) => void): () => void {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, new Set());
    }
    this.eventListeners.get(eventType)!.add(listener);

    // Return unsubscribe function
    return () => {
      this.eventListeners.get(eventType)?.delete(listener);
    };
  }

  /**
   * Cleanup expired entries
   */
  async cleanup(): Promise<number> {
    let cleaned = 0;
    try {
      const keys = await this.storage.keys();
      const now = Date.now();

      for (const key of keys) {
        try {
          const entry = await this.storage.get<CachedClipsData>(key);
          if (!entry) {
            continue;
          }

          const age = now - entry._metadata.cachedAt;
          if (age >= this.config.ttl) {
            await this.storage.remove(key);
            cleaned++;
          }
        } catch (error) {
          // Remove corrupted entries
          this.logger.warn("Removing corrupted cache entry", { key, error });
          await this.storage.remove(key).catch(() => {
            // Ignore removal errors
          });
          cleaned++;
        }
      }

      await this.refreshStats();
      this.emitEvent("cleanup", undefined, { cleaned });
      return cleaned;
    } catch (error) {
      this.recordError();
      this.logger.error("Failed to cleanup cache", { error });
      return cleaned;
    }
  }

  /**
   * Destroy cache manager and cleanup resources
   */
  destroy(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
    this.eventListeners.clear();
  }

  /**
   * Get cache key for video ID
   */
  private getCacheKey(videoId: string): string {
    return `clips_${videoId}`;
  }

  /**
   * Estimate size of cache entry in bytes
   */
  private estimateEntrySize(entry: CachedClipsData): number {
    try {
      const serialized = JSON.stringify(entry);
      // UTF-16 encoding: 2 bytes per character
      return serialized.length * 2;
    } catch {
      // Fallback estimation
      return 1024; // 1KB default
    }
  }

  /**
   * Evict entries if needed to make space
   */
  private async evictIfNeeded(newEntrySize: number): Promise<void> {
    if (!this.config.enableLRU) {
      return;
    }

    const currentSize = this.stats.totalSize;
    const currentEntries = this.stats.totalEntries;

    // Check if we need to evict based on size
    if (currentSize + newEntrySize > this.config.maxSizeBytes) {
      const targetSize = this.config.maxSizeBytes - newEntrySize;
      await this.evictToSize(targetSize);
    }

    // Check if we need to evict based on entry count
    if (currentEntries >= this.config.maxEntries) {
      const toEvict = currentEntries - this.config.maxEntries + 1;
      await this.evictOldest(toEvict);
    }
  }

  /**
   * Evict oldest entries by access time
   */
  private async evictOldest(count: number): Promise<void> {
    try {
      const keys = await this.storage.keys();
      const entries: Array<{ key: string; lastAccessed: number }> = [];

      for (const key of keys) {
        try {
          const entry = await this.storage.get<CachedClipsData>(key);
          if (entry) {
            entries.push({
              key,
              lastAccessed: entry._metadata.lastAccessedAt,
            });
          }
        } catch {
          // Skip corrupted entries
        }
      }

      // Sort by last accessed time (oldest first)
      entries.sort((a, b) => a.lastAccessed - b.lastAccessed);

      // Evict oldest entries
      const toEvict = entries.slice(0, count);
      for (const { key } of toEvict) {
        await this.storage.remove(key);
        this.stats.evictions++;
      }

      await this.refreshStats();
    } catch (error) {
      this.logger.error("Failed to evict oldest entries", { error });
      throw error;
    }
  }

  /**
   * Evict entries until total size is below target
   */
  private async evictToSize(targetSize: number): Promise<void> {
    try {
      const keys = await this.storage.keys();
      const entries: Array<{ key: string; size: number; lastAccessed: number }> = [];

      for (const key of keys) {
        try {
          const size = await this.storage.size(key);
          const entry = await this.storage.get<CachedClipsData>(key);
          if (entry) {
            entries.push({
              key,
              size,
              lastAccessed: entry._metadata.lastAccessedAt,
            });
          }
        } catch {
          // Skip corrupted entries
        }
      }

      // Sort by last accessed time (oldest first)
      entries.sort((a, b) => a.lastAccessed - b.lastAccessed);

      // Evict until we're under target size
      let currentSize = entries.reduce((sum, e) => sum + e.size, 0);
      for (const { key, size } of entries) {
        if (currentSize <= targetSize) {
          break;
        }
        await this.storage.remove(key);
        currentSize -= size;
        this.stats.evictions++;
      }

      await this.refreshStats();
    } catch (error) {
      this.logger.error("Failed to evict to target size", { error });
      throw error;
    }
  }

  /**
   * Refresh cache statistics
   */
  private async refreshStats(): Promise<void> {
    try {
      const keys = await this.storage.keys();
      let totalSize = 0;

      for (const key of keys) {
        try {
          totalSize += await this.storage.size(key);
        } catch {
          // Skip errors for individual entries
        }
      }

      this.stats.totalEntries = keys.length;
      this.stats.totalSize = totalSize;
    } catch (error) {
      this.logger.warn("Failed to refresh cache stats", { error });
    }
  }

  /**
   * Record cache hit
   */
  private recordHit(): void {
    this.stats.hits++;
  }

  /**
   * Record cache miss
   */
  private recordMiss(): void {
    this.stats.misses++;
  }

  /**
   * Record error
   */
  private recordError(): void {
    this.stats.errors++;
  }

  /**
   * Emit cache event
   */
  private emitEvent(
    type: CacheEvent["type"],
    key?: string,
    metadata?: Record<string, unknown>
  ): void {
    const event: CacheEvent = {
      type,
      key,
      timestamp: Date.now(),
      metadata,
    };

    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.forEach((listener) => {
        try {
          listener(event);
        } catch (error) {
          this.logger.warn("Cache event listener error", { type, error });
        }
      });
    }
  }

  /**
   * Start periodic cleanup timer
   */
  private startCleanupTimer(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
    }

    this.cleanupTimer = setInterval(() => {
      this.cleanup().catch((error) => {
        this.logger.error("Periodic cleanup failed", { error });
      });
    }, this.config.cleanupInterval);
  }
}
