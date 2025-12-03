# Firestore Migration Plan

## Executive Summary

**Problem**: Current system relies heavily on R2 file listing and filename parsing, which is slow, fragile, and doesn't scale.

**Solution**: Use Firestore for metadata storage and queries, R2 only for file storage and presigned URL generation.

**Expected Improvement**: 5-10x faster API responses, better scalability, more reliable.

## Architecture Overview

```
┌─────────────────┐
│   Frontend      │
│   (Next.js)     │
└────────┬────────┘
         │
         │ Fast Queries (50-200ms)
         ▼
┌─────────────────┐
│   Firestore     │
│   (Metadata)    │
│                 │
│ • Videos        │
│ • Clips         │
│ • Statistics    │
└─────────────────┘
         │
         │ On-demand URLs
         ▼
┌─────────────────┐
│   Cloudflare R2 │
│   (Files Only)  │
│                 │
│ • Video files   │
│ • Thumbnails   │
│ • highlights.json│
└─────────────────┘
```

## Implementation Steps

### Step 1: Create Repositories ✅
- [x] `ClipRepository` - Manage clip metadata
- [x] `VideoRepository` - Manage video metadata
- [x] Schema documentation

### Step 2: Update Workflow ✅
- [x] Add Firestore writes in `process_clip()`
- [x] Update video creation to use `VideoRepository`
- [x] Update statistics calculation

### Step 3: Update API Endpoints ✅
- [x] Update `get_video_info()` to use Firestore
- [x] Keep R2 only for presigned URLs
- [x] Add fallback to R2 for migration period

### Step 4: Update Reprocessing ✅
- [x] Write clip metadata during reprocessing (via process_clip)
- [x] Update statistics after reprocessing

### Step 5: Migration Script
- [ ] Script to migrate existing videos
- [ ] Validation and verification

### Step 6: Cleanup
- [ ] Remove R2 listing logic (after migration period)
- [ ] Remove filename parsing (after migration period)
- [x] Update documentation

## Code Changes Required

### 1. Workflow Changes (`app/core/workflow.py`)

```python
# In process_clip() after upload
from app.core.clips_repository import ClipRepository

if context.user_id is not None:
    clips_repo = ClipRepository(context.user_id, context.run_id)
    
    # Get highlight info from task context
    highlight = get_highlight_for_clip(task, context)
    
    clip_id = task.filename.rsplit('.', 1)[0]
    file_size = task.out_path.stat().st_size
    
    clips_repo.create_clip(
        clip_id=clip_id,
        filename=task.filename,
        scene_id=highlight.get("id", 0),
        scene_title=highlight.get("title", ""),
        style=task.style,
        start_time=task.start,
        end_time=task.end,
        duration_seconds=calculate_duration(task.start, task.end),
        file_size_bytes=file_size,
        has_thumbnail=thumb_path.exists(),
    )
    
    clips_repo.update_clip_status(clip_id, "completed")
```

### 2. API Endpoint Changes (`app/routers/videos.py`)

```python
from app.core.clips_repository import ClipRepository

@router.get("/videos/{video_id}")
async def get_video_info(...):
    # ... validation ...
    
    # Get clips from Firestore (fast!)
    clips_repo = ClipRepository(uid, video_id)
    clip_docs = clips_repo.list_clips(status="completed")
    
    # Convert to response format
    clips = []
    for clip_doc in clip_docs:
        clips.append({
            "name": clip_doc["filename"],
            "title": clip_doc["scene_title"],
            "description": clip_doc.get("scene_description", ""),
            "url": f"/api/videos/{video_id}/clips/{clip_doc['filename']}",
            "thumbnail": (
                generate_presigned_url(clip_doc["thumbnail_r2_key"])
                if clip_doc.get("has_thumbnail") else None
            ),
            "size": f"{clip_doc['file_size_mb']:.1f} MB",
            "style": clip_doc["style"],
        })
    
    return VideoInfoResponse(id=video_id, clips=clips, ...)
```

## Firestore Indexes Required

Create these indexes in Firestore console:

1. **Videos Collection**
   - Collection: `users/{uid}/videos`
   - Fields: `created_at` (Descending)

2. **Clips Collection**  
   - Collection: `users/{uid}/videos/{video_id}/clips`
   - Fields: `status`, `created_at` (Ascending)

3. **Style Filter**
   - Collection: `users/{uid}/videos/{video_id}/clips`
   - Fields: `style`, `created_at` (Ascending)

## Testing Checklist

- [ ] New videos write to Firestore correctly
- [ ] Clips metadata is accurate
- [ ] API endpoints return correct data
- [ ] Presigned URLs work correctly
- [ ] Statistics are accurate
- [ ] Reprocessing writes to Firestore
- [ ] Migration script works correctly
- [ ] Performance improvement verified

## Rollback Plan

If issues arise:
1. Keep dual-write enabled
2. Switch API endpoints back to R2 reading
3. Fix Firestore issues
4. Re-enable Firestore reading

## Monitoring

Track these metrics:
- API response times (should decrease)
- Firestore read/write operations
- R2 API calls (should decrease)
- Error rates
- Cache hit rates

## Timeline

- **Week 1**: Implement repositories and workflow updates
- **Week 2**: Update API endpoints with fallback
- **Week 3**: Migration script and testing
- **Week 4**: Full rollout and cleanup

