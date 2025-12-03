# Cache Module

Production-ready caching system for clips data with LRU eviction, versioning, and comprehensive error handling.

## Architecture

### Design Principles

- **SOLID**: Single Responsibility, Open/Closed, Dependency Inversion
- **DRY**: Reusable storage adapters and cache managers
- **Security**: Key sanitization, XSS prevention, quota handling
- **Performance**: LRU eviction, size limits, debounced cleanup
- **Observability**: Event system, statistics, logging

### Components

1. **Types** (`types.ts`): Centralized type definitions
2. **Storage Adapter** (`storage/localStorageAdapter.ts`): Abstraction for storage operations
3. **Cache Manager** (`CacheManager.ts`): Core caching logic with LRU eviction
4. **Public API** (`index.ts`): Singleton instance and convenience functions
5. **React Hook** (`useClipsCache.ts`): React-friendly hook interface

## Features

### ✅ LRU Eviction
- Automatically evicts least-recently-used entries when limits are reached
- Configurable max entries and max size

### ✅ Cache Versioning
- Schema versioning for cache migrations
- Automatic invalidation of incompatible cache entries

### ✅ Error Handling
- Graceful degradation on storage errors
- Automatic cleanup of corrupted entries
- Quota exceeded handling with eviction

### ✅ Observability
- Cache statistics (hits, misses, evictions)
- Event system for monitoring
- Comprehensive logging

### ✅ Security
- Key sanitization to prevent XSS
- Namespaced keys to prevent collisions
- Safe error handling

## Usage

### Basic Usage

```typescript
import { getCachedClips, setCachedClips, invalidateClipsCache } from "@/lib/cache";

// Get cached data
const cached = await getCachedClips(videoId);
if (cached) {
  // Use cached data
  setClips(cached.clips);
}

// Cache data
await setCachedClips(videoId, {
  clips: clipsData,
  custom_prompt: prompt,
  video_title: title,
  video_url: url,
});

// Invalidate cache
await invalidateClipsCache(videoId);
```

### Using React Hook

```typescript
import { useClipsCache } from "@/lib/cache/useClipsCache";

function MyComponent() {
  const cache = useClipsCache();
  
  const loadData = async (videoId: string) => {
    const cached = await cache.get(videoId);
    if (!cached) {
      // Fetch from API
      const data = await fetchClips(videoId);
      await cache.set(videoId, data);
    }
  };
}
```

### Observability

```typescript
import { getCacheStats, onCacheEvent } from "@/lib/cache";

// Get statistics
const stats = getCacheStats();
console.log(`Cache hits: ${stats.hits}, misses: ${stats.misses}`);

// Subscribe to events
const unsubscribe = onCacheEvent("hit", (event) => {
  console.log("Cache hit:", event.key);
});
```

## Configuration

Default configuration (can be customized in `index.ts`):

- **TTL**: 24 hours
- **Max Entries**: 100
- **Max Size**: 10MB
- **Version**: 1
- **Cleanup Interval**: 1 hour

## Migration

When updating the cache schema:

1. Increment `version` in `CacheManager` constructor
2. Old cache entries will be automatically invalidated
3. New entries will use the updated schema

## Testing

The modular architecture makes testing easy:

- Mock `IStorageAdapter` for unit tests
- Test `CacheManager` independently
- Test storage adapters separately

## Performance Considerations

- Cache operations are async but non-blocking
- LRU eviction prevents unbounded growth
- Periodic cleanup runs in background
- Size estimation prevents quota errors

