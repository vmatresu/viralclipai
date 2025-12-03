# Firestore Performance Optimization Guide

## Query Optimization

### 1. Use Indexes
Firestore requires composite indexes for queries with multiple filters or ordering.

**Required Indexes:**
```javascript
// firestore.indexes.json
{
  "indexes": [
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        { "fieldPath": "status", "order": "ASCENDING" },
        { "fieldPath": "priority", "order": "ASCENDING" },
        { "fieldPath": "created_at", "order": "ASCENDING" }
      ]
    },
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        { "fieldPath": "style", "order": "ASCENDING" },
        { "fieldPath": "created_at", "order": "ASCENDING" }
      ]
    },
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        { "fieldPath": "scene_id", "order": "ASCENDING" },
        { "fieldPath": "created_at", "order": "ASCENDING" }
      ]
    },
    {
      "collectionGroup": "videos",
      "queryScope": "COLLECTION",
      "fields": [
        { "fieldPath": "status", "order": "ASCENDING" },
        { "fieldPath": "created_at", "order": "DESCENDING" }
      ]
    }
  ]
}
```

### 2. Limit Query Results
Always use `limit()` when possible to reduce data transfer:

```python
# Good: Limited query
clips = clips_repo.list_clips(status="completed", limit=100)

# Bad: No limit (could return thousands)
clips = clips_repo.list_clips(status="completed")
```

### 3. Use Field Selection
Firestore allows selecting specific fields to reduce data transfer:

```python
# Future optimization: Select only needed fields
query = collection.select(["filename", "style", "status"])
```

### 4. Batch Operations
Use batch writes for multiple operations:

```python
# Good: Batch write (up to 500 operations)
clips_repo.create_clips_batch(clips_list)

# Bad: Individual writes
for clip in clips_list:
    clips_repo.create_clip(clip)
```

## Caching Strategy

### 1. Application-Level Caching
Cache frequently accessed data:

```python
from functools import lru_cache
from app.core.cache import get_video_info_cache

cache = get_video_info_cache()

@lru_cache(maxsize=100)
def get_video_cached(video_id: str):
    return video_repo.get_video(video_id)
```

### 2. Cache Invalidation
Invalidate cache on updates:

```python
# After updating video
video_repo.update_video_status(video_id, "completed")
cache.invalidate(f"{user_id}:{video_id}")
```

### 3. Firestore Real-time Listeners
Use Firestore listeners for real-time updates (optional):

```python
# Listen for clip updates
def on_clip_snapshot(doc_snapshot, changes, read_time):
    for change in changes:
        if change.type.name == 'ADDED':
            # New clip added
            pass
```

## Performance Benchmarks

### Expected Performance

| Operation | R2-based | Firestore-based | Improvement |
|-----------|----------|-----------------|-------------|
| List 10 clips | 500ms | 50ms | **10x** |
| List 100 clips | 2000ms | 100ms | **20x** |
| Filter by style | 2000ms | 80ms | **25x** |
| Count clips | 1500ms | 30ms | **50x** |
| Get video info | 3000ms | 150ms | **20x** |

### Factors Affecting Performance

1. **Indexes**: Without proper indexes, queries can be slow
2. **Document Size**: Keep documents small (< 1MB)
3. **Query Complexity**: Simpler queries are faster
4. **Network Latency**: Firestore is faster than R2 for metadata
5. **Caching**: Application-level caching improves repeat queries

## Best Practices

### 1. Write Path
```python
# 1. Create metadata in Firestore (fast)
clip_metadata = ClipMetadata(...)
clips_repo.create_clip(clip_metadata)

# 2. Upload file to R2 (slow, but necessary)
upload_to_r2(file_path, r2_key)

# 3. Update status in Firestore
clips_repo.update_clip_status(clip_id, "completed")
```

### 2. Read Path
```python
# 1. Query metadata from Firestore (fast)
clips = clips_repo.list_clips(status="completed")

# 2. Generate presigned URLs on-demand (only when needed)
for clip in clips:
    url = generate_presigned_url(clip.r2_key)  # Only for viewing
```

### 3. Error Handling
```python
try:
    clips_repo.create_clip(clip_metadata)
except ClipRepositoryError as e:
    logger.error(f"Failed to create clip: {e}")
    # Mark clip as failed, don't break entire workflow
    clips_repo.update_clip_status(clip_id, "failed")
```

### 4. Transactions
Use transactions for atomic operations:

```python
with clips_repo.transaction() as transaction:
    clips_repo.create_clips_batch(clips, transaction=transaction)
    video_repo.update_clip_statistics(video_id, transaction=transaction)
    transaction.commit()  # All or nothing
```

## Monitoring

Track these metrics:
- **Query Latency**: Average time for Firestore queries
- **Write Latency**: Average time for Firestore writes
- **Error Rate**: Percentage of failed operations
- **Cache Hit Rate**: Percentage of cache hits
- **Document Count**: Number of documents per collection

## Troubleshooting

### Slow Queries
1. Check if indexes are created
2. Verify query complexity
3. Check document size
4. Review query patterns

### High Costs
1. Monitor read/write operations
2. Use caching to reduce reads
3. Batch operations to reduce writes
4. Use field selection to reduce data transfer

### Consistency Issues
1. Use transactions for related operations
2. Implement retry logic for failed operations
3. Monitor error logs for patterns

