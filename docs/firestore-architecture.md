# Firestore Architecture Guide

## Overview

This document describes the production-ready Firestore-based architecture that replaces R2 file listing with a fast, scalable, and maintainable metadata layer. The system separates metadata storage (Firestore) from file storage (R2) for optimal performance and maintainability.

## Architecture Principles

- **Firestore for Metadata**: Fast queries, indexing, real-time updates
- **R2 for Files**: Large file storage, presigned URLs for secure access
- **Separation of Concerns**: Metadata separate from file storage
- **Type Safety**: Pydantic models with runtime validation
- **Repository Pattern**: Clean, testable data access layer

## Collection Structure

```
users/{uid}/
  ├── videos/{video_id}          # Video metadata
  └── clips/{clip_id}            # Clip metadata (subcollection)
```

## Schema Definitions

### Video Document

**Path**: `users/{uid}/videos/{video_id}`

```typescript
{
  // Identifiers
  video_id: string;              // Unique video ID (run_id)
  user_id: string;              // Owner user ID

  // Video Information
  video_url: string;            // Source YouTube URL
  video_title: string;           // Video title
  youtube_id: string;            // Extracted YouTube ID

  // Processing Status
  status: "processing" | "completed" | "failed";
  created_at: Timestamp;
  completed_at?: Timestamp;
  failed_at?: Timestamp;
  error_message?: string;

  // Highlights Metadata
  highlights_count: number;    // Number of scenes detected
  highlights_summary?: {        // Quick summary
    total_duration: number;     // Total duration in seconds
    categories: string[];        // Unique hook categories
  };

  // Processing Configuration
  custom_prompt?: string;        // Custom prompt used
  styles_processed: string[];   // Styles that were processed
  crop_mode: string;            // Crop mode used
  target_aspect: string;        // Target aspect ratio

  // Statistics
  clips_count: number;          // Total clips generated
  clips_by_style: {             // Clips grouped by style
    [style: string]: number;
  };

  // Storage References
  highlights_json_key: string;  // R2 key: "{uid}/{video_id}/highlights.json"

  // Metadata
  created_by: string;           // User ID
  updated_at: Timestamp;
}
```

### Clip Document

**Path**: `users/{uid}/videos/{video_id}/clips/{clip_id}`

```typescript
{
  // Identifiers
  clip_id: string;              // Unique clip ID (filename without extension)
  video_id: string;             // Parent video ID
  user_id: string;              // Owner user ID

  // Scene Information
  scene_id: number;             // Highlight/scene ID from highlights.json
  scene_title: string;          // Scene title
  scene_description?: string;   // Scene description

  // Clip Metadata
  filename: string;             // R2 filename: "clip_XX_XX_title_style.mp4"
  style: string;                // Style: "split", "left_focus", etc.
  priority: number;             // Processing priority

  // Timing Information
  start_time: string;           // "HH:MM:SS" format
  end_time: string;             // "HH:MM:SS" format
  duration_seconds: number;      // Clip duration

  // File Information
  file_size_bytes: number;       // File size in bytes
  file_size_mb: number;          // File size in MB (for display)
  has_thumbnail: boolean;        // Whether thumbnail exists

  // Storage References
  r2_key: string;               // R2 key: "{uid}/{video_id}/clips/{filename}"
  thumbnail_r2_key?: string;    // R2 key for thumbnail

  // Status
  status: "processing" | "completed" | "failed";
  created_at: Timestamp;
  completed_at?: Timestamp;

  // Metadata
  created_by: string;           // User ID
}
```

## Integration Examples

### Creating Clip Metadata

```python
from datetime import datetime, timezone
from app.core.repositories.clips import ClipRepository
from app.core.repositories.models import ClipMetadata

async def process_clip_example(
    user_id: str,
    video_id: str,
    task: "ClipTask",
    highlight: dict,
    file_path: Path,
    thumb_path: Optional[Path],
) -> None:
    clips_repo = ClipRepository(user_id, video_id)

    # Extract clip information
    clip_id = task.filename.rsplit('.', 1)[0]  # Remove .mp4 extension
    file_size = file_path.stat().st_size if file_path.exists() else 0
    has_thumbnail = thumb_path.exists() if thumb_path else False

    # Calculate duration
    duration = calculate_duration_seconds(task.start, task.end)

    # Create metadata model
    clip_metadata = ClipMetadata(
        clip_id=clip_id,
        video_id=video_id,
        user_id=user_id,
        scene_id=highlight.get("id", 0),
        scene_title=highlight.get("title", ""),
        scene_description=highlight.get("description"),
        filename=task.filename,
        style=task.style,
        priority=highlight.get("priority", 99),
        start_time=task.start,
        end_time=task.end,
        duration_seconds=duration,
        file_size_bytes=file_size,
        has_thumbnail=has_thumbnail,
        r2_key=f"{user_id}/{video_id}/clips/{task.filename}",
        thumbnail_r2_key=(
            f"{user_id}/{video_id}/clips/{thumb_path.name}"
            if has_thumbnail and thumb_path
            else None
        ),
        status="processing",
        created_at=datetime.now(timezone.utc),
        created_by=user_id,
    )

    # Create in Firestore
    try:
        clips_repo.create_clip(clip_metadata)

        # After R2 upload completes, update status
        clips_repo.update_clip_status(
            clip_id=clip_id,
            status="completed",
            file_size_bytes=file_size,
            has_thumbnail=has_thumbnail,
        )
    except ClipRepositoryError as e:
        logger.error(f"Failed to create clip metadata: {e}")
        clips_repo.update_clip_status(clip_id=clip_id, status="failed")
```

### Creating Video Metadata

```python
from app.core.repositories.videos import VideoRepository
from app.core.repositories.models import VideoMetadata

async def create_video_example(
    user_id: str,
    video_id: str,
    video_url: str,
    video_title: str,
    youtube_id: str,
    highlights: list,
    styles_processed: list,
    custom_prompt: Optional[str] = None,
) -> None:
    video_repo = VideoRepository(user_id)

    # Calculate highlights summary
    highlights_summary = {
        "total_duration": sum(h.get("duration", 0) for h in highlights),
        "categories": list(set(
            h.get("hook_category")
            for h in highlights
            if h.get("hook_category")
        )),
    }

    # Create metadata model
    video_metadata = VideoMetadata(
        video_id=video_id,
        user_id=user_id,
        video_url=video_url,
        video_title=video_title,
        youtube_id=youtube_id,
        status="processing",
        created_at=datetime.now(timezone.utc),
        updated_at=datetime.now(timezone.utc),
        highlights_count=len(highlights),
        highlights_summary=highlights_summary,
        custom_prompt=custom_prompt,
        styles_processed=styles_processed,
        crop_mode="none",
        target_aspect="9:16",
        clips_count=0,
        clips_by_style={},
        highlights_json_key=f"{user_id}/{video_id}/highlights.json",
        created_by=user_id,
    )

    # Create in Firestore
    video_repo.create_or_update_video(video_metadata)
```

### Querying Clips (Replace R2 Listing)

```python
async def get_video_clips_example(
    user_id: str,
    video_id: str,
    style: Optional[str] = None,
) -> list:
    clips_repo = ClipRepository(user_id, video_id)

    # Query clips from Firestore (fast!)
    clips = clips_repo.list_clips(
        status="completed",
        style=style,  # Optional filter
        order_by="priority",
    )

    # Convert to API response format
    result = []
    for clip in clips:
        result.append({
            "name": clip.filename,
            "title": clip.scene_title,
            "description": clip.scene_description or "",
            "url": f"/api/videos/{video_id}/clips/{clip.filename}",
            "thumbnail": (
                generate_presigned_url(clip.thumbnail_r2_key)
                if clip.has_thumbnail and clip.thumbnail_r2_key
                else None
            ),
            "size": f"{clip.file_size_mb:.1f} MB",
            "style": clip.style,
        })

    return result
```

### API Endpoint Integration

```python
from app.core.repositories.clips import ClipRepository
from app.core.storage import generate_presigned_url

@router.get("/videos/{video_id}", response_model=VideoInfoResponse)
async def get_video_info(uid: str, video_id: str):
    # Get clips from Firestore (fast!)
    clips_repo = ClipRepository(uid, video_id)
    clip_docs = clips_repo.list_clips(status="completed")

    # Convert to response format with presigned URLs
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
        # ... other fields
    )
```

## Performance Benefits

### Before (R2-based)
- **List Clips**: ~500-2000ms (paginated R2 list + parse filenames)
- **Get Video Info**: ~1000-3000ms (load highlights.json + list clips)
- **Filter by Style**: ~1000-3000ms (list all + filter in memory)

### After (Firestore-based)
- **List Clips**: ~50-200ms (single Firestore query, indexed)
- **Get Video Info**: ~100-300ms (Firestore query + highlights.json only if needed)
- **Filter by Style**: ~50-200ms (indexed Firestore query)

**Expected Performance Improvement: 5-10x faster**

## Error Handling

The repository layer provides comprehensive error handling:

```python
from app.core.repositories.exceptions import (
    ClipRepositoryError,
    VideoRepositoryError,
    NotFoundError,
    ValidationError,
    ConflictError
)

try:
    clips_repo.create_clip(clip_metadata)
except ValidationError as e:
    # Invalid data provided
    logger.error(f"Invalid clip data: {e}")
except ConflictError as e:
    # Clip already exists
    logger.error(f"Clip already exists: {e}")
except ClipRepositoryError as e:
    # General repository error
    logger.error(f"Repository error: {e}")
```

## Transactions

Use transactions for atomic operations:

```python
with clips_repo.transaction() as transaction:
    clips_repo.create_clips_batch(clips, transaction=transaction)
    video_repo.update_clip_statistics(video_id, transaction=transaction)
    transaction.commit()  # All or nothing
```

## Migration Completed ✅

The migration to Firestore has been completed with the following components:

- ✅ Repository layer (`app/core/repositories/`)
- ✅ Type-safe models with Pydantic validation
- ✅ Error handling with custom exception hierarchy
- ✅ Integration into workflow and API endpoints
- ✅ Firestore indexes configured
- ✅ Performance optimizations implemented

## Files

- `app/core/repositories/__init__.py` - Package exports
- `app/core/repositories/exceptions.py` - Exception hierarchy
- `app/core/repositories/models.py` - Pydantic models
- `app/core/repositories/clips.py` - Clip repository
- `app/core/repositories/videos.py` - Video repository

This architecture follows SOLID principles, DRY methodology, and modern Python best practices for a production-ready, scalable system.
