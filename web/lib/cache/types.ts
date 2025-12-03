/**
 * Cache Types & Interfaces
 * 
 * Centralized type definitions for the caching system.
 */

import type { Clip } from "@/components/ClipGrid";

/**
 * Cache metadata for tracking entry information
 */
export interface CacheMetadata {
  /** Timestamp when the entry was cached (Unix epoch in milliseconds) */
  cachedAt: number;
  /** Timestamp when the entry was last accessed (Unix epoch in milliseconds) */
  lastAccessedAt: number;
  /** Number of times this entry has been accessed */
  accessCount: number;
  /** Cache schema version for migration support */
  version: number;
}

/**
 * Cached clips data structure
 */
export interface CachedClipsData {
  /** Array of clip objects */
  clips: Clip[];
  /** Custom prompt used for video processing */
  custom_prompt?: string | null;
  /** Title of the video */
  video_title?: string | null;
  /** URL of the original video */
  video_url?: string | null;
  /** Cache metadata */
  _metadata: CacheMetadata;
}

/**
 * Cache configuration options
 */
export interface CacheConfig {
  /** Time to live in milliseconds (default: 24 hours) */
  ttl: number;
  /** Maximum number of cache entries (default: 100) */
  maxEntries: number;
  /** Maximum total size in bytes (default: 10MB) */
  maxSizeBytes: number;
  /** Cache schema version (increment for migrations) */
  version: number;
  /** Enable LRU eviction when max entries reached */
  enableLRU: boolean;
  /** Cleanup interval in milliseconds (default: 1 hour) */
  cleanupInterval: number;
}

/**
 * Cache entry with size information
 */
export interface CacheEntry<T> {
  key: string;
  value: T;
  size: number;
  metadata: CacheMetadata;
}

/**
 * Cache statistics for observability
 */
export interface CacheStats {
  /** Total number of entries in cache */
  totalEntries: number;
  /** Total size in bytes */
  totalSize: number;
  /** Number of cache hits */
  hits: number;
  /** Number of cache misses */
  misses: number;
  /** Number of entries evicted */
  evictions: number;
  /** Number of errors encountered */
  errors: number;
}

/**
 * Storage adapter interface for abstraction
 */
export interface IStorageAdapter {
  get<T>(key: string): Promise<T | null>;
  set<T>(key: string, value: T): Promise<void>;
  remove(key: string): Promise<void>;
  clear(): Promise<void>;
  keys(): Promise<string[]>;
  size(key: string): Promise<number>;
}

/**
 * Cache event types for observability
 */
export type CacheEventType = 
  | "hit"
  | "miss"
  | "set"
  | "evict"
  | "invalidate"
  | "error"
  | "cleanup";

export interface CacheEvent {
  type: CacheEventType;
  key?: string;
  timestamp: number;
  metadata?: Record<string, unknown>;
}

