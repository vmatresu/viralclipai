# Firestore Integration Guide

This guide explains how to integrate Firestore metadata storage into the video processing workflow.

## Integration Points

### 1. Clip Creation (Workflow)

When a clip is created and uploaded to R2, also create metadata in Firestore:

```python
from app.core.clips_repository import ClipRepository

# In process_clip() function
clips_repo = ClipRepository(context.user_id, context.run_id)

# After clip is rendered and uploaded
clip_id = task.filename.rsplit('.', 1)[0]  # Remove .mp4 extension
file_size = task.out_path.stat().st_size if task.out_path.exists() else 0
has_thumbnail = thumb_path.exists() if thumb_path else False

# Extract scene info from highlight
highlight = get_highlight_for_task(task)  # Get highlight from context
scene_id = highlight.get("id", 0)
scene_title = highlight.get("title", "")
scene_description = highlight.get("description")

clips_repo.create_clip(
    clip_id=clip_id,
    filename=task.filename,
    scene_id=scene_id,
    scene_title=scene_title,
    style=task.style,
    start_time=task.start,
    end_time=task.end,
    duration_seconds=calculate_duration(task.start, task.end),
    priority=highlight.get("priority", 99),
    scene_description=scene_description,
    file_size_bytes=file_size,
    has_thumbnail=has_thumbnail,
)

# After upload completes, update status
clips_repo.update_clip_status(
    clip_id=clip_id,
    status="completed",
    file_size_bytes=file_size,
    has_thumbnail=has_thumbnail,
)
```

### 2. Video Info Endpoint

Replace R2 listing with Firestore query:

```python
from app.core.clips_repository import ClipRepository
from app.core.storage import generate_presigned_url

@router.get("/videos/{video_id}", response_model=VideoInfoResponse)
async def get_video_info(...):
    # ... existing validation ...
    
    # Get clips from Firestore (fast!)
    clips_repo = ClipRepository(uid, video_id)
    clip_docs = clips_repo.list_clips(status="completed")
    
    # Convert to API response format with presigned URLs
    clips = []
    for clip_doc in clip_docs:
        # Generate presigned URL only when needed (on-demand)
        clip_url = f"/api/videos/{video_id}/clips/{clip_doc['filename']}"
        
        thumbnail_url = None
        if clip_doc.get("has_thumbnail"):
            thumbnail_url = generate_presigned_url(
                clip_doc["thumbnail_r2_key"],
                expires_in=3600
            )
        
        clips.append({
            "name": clip_doc["filename"],
            "title": clip_doc["scene_title"],
            "description": clip_doc.get("scene_description", ""),
            "url": clip_url,
            "thumbnail": thumbnail_url,
            "size": f"{clip_doc['file_size_mb']:.1f} MB",
            "style": clip_doc["style"],
        })
    
    return VideoInfoResponse(
        id=video_id,
        clips=clips,
        ...
    )
```

### 3. Video Creation

Update video creation to use new repository:

```python
from app.core.video_repository import VideoRepository

# In workflow after highlights are extracted
video_repo = VideoRepository(user_id)

# Calculate highlights summary
highlights_summary = {
    "total_duration": sum(h.get("duration", 0) for h in highlights),
    "categories": list(set(h.get("hook_category") for h in highlights if h.get("hook_category"))),
}

video_repo.create_or_update_video(
    video_id=run_id,
    video_url=url,
    video_title=final_title,
    youtube_id=youtube_id,
    status="processing",
    custom_prompt=custom_prompt,
    styles_processed=styles_to_process,
    crop_mode=crop_mode,
    target_aspect=target_aspect,
    highlights_count=len(highlights),
    highlights_summary=highlights_summary,
)
```

### 4. Statistics Update

After all clips are processed, update video statistics:

```python
# At end of workflow
video_repo.update_clip_statistics(run_id)
```

## Performance Comparison

### Current (R2-based)
```
GET /api/videos/{video_id}
├── Load highlights.json from R2: ~200ms
├── List all clips from R2: ~500-2000ms
├── Parse filenames: ~50ms
├── Generate presigned URLs: ~100ms (for all clips)
└── Total: ~850-2350ms
```

### New (Firestore-based)
```
GET /api/videos/{video_id}
├── Query clips from Firestore: ~50-200ms
├── Generate presigned URLs (on-demand): ~0ms (deferred)
└── Total: ~50-200ms

Improvement: 5-10x faster
```

## Migration Strategy

### Phase 1: Dual Write (Current)
- Write to both Firestore and R2
- Read from R2 (existing behavior)
- Monitor for any issues

### Phase 2: Firestore Read with Fallback
- Read from Firestore first
- Fallback to R2 if Firestore data missing
- Log any fallbacks for monitoring

### Phase 3: Data Migration
- Migrate existing videos to Firestore
- Verify data consistency
- Remove R2 listing logic

### Phase 4: R2 Only for Files
- Use R2 only for file storage
- Use Firestore for all metadata queries
- Remove filename parsing logic

## Benefits

1. **Performance**: 5-10x faster queries
2. **Scalability**: Handles thousands of clips efficiently
3. **Queryability**: Filter by style, scene, status
4. **Reliability**: No filename parsing errors
5. **Real-time**: Firestore real-time listeners for live updates
6. **Cost**: Fewer R2 API calls (list operations are expensive)

