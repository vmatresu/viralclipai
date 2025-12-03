"""
Example integration of Firestore repositories into workflow.

This demonstrates best practices for using the repository layer in production code.
"""

from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from app.core.repositories.clips import ClipRepository
from app.core.repositories.exceptions import ClipRepositoryError, NotFoundError
from app.core.repositories.models import ClipMetadata
from app.core.repositories.videos import VideoRepository
from app.core.repositories.models import VideoMetadata


async def process_clip_example(
    user_id: str,
    video_id: str,
    task: "ClipTask",  # From workflow
    highlight: dict,
    file_path: Path,
    thumb_path: Optional[Path],
) -> None:
    """
    Example: Create clip metadata in Firestore after rendering.
    
    This should be called after:
    1. Clip is rendered to file_path
    2. Thumbnail is generated (if applicable)
    3. Files are uploaded to R2
    """
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
        status="processing",  # Will be updated to "completed" after upload
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
        # Log error and mark as failed
        logger.error(f"Failed to create clip metadata: {e}")
        clips_repo.update_clip_status(clip_id=clip_id, status="failed")


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
    """
    Example: Create video metadata in Firestore.
    
    This should be called after highlights are extracted.
    """
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
        crop_mode="none",  # From context
        target_aspect="9:16",  # From context
        clips_count=0,  # Will be updated as clips are created
        clips_by_style={},  # Will be updated as clips are created
        highlights_json_key=f"{user_id}/{video_id}/highlights.json",
        created_by=user_id,
    )
    
    # Create in Firestore
    video_repo.create_or_update_video(video_metadata)


async def get_video_clips_example(
    user_id: str,
    video_id: str,
    style: Optional[str] = None,
) -> list:
    """
    Example: Get clips for a video (replaces R2 listing).
    
    This is much faster than listing R2 files.
    """
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


async def update_statistics_example(
    user_id: str,
    video_id: str,
) -> None:
    """
    Example: Update video statistics after all clips are processed.
    
    This aggregates clip statistics and updates the video document.
    """
    video_repo = VideoRepository(user_id)
    
    # This will:
    # 1. Query all completed clips
    # 2. Count total clips
    # 3. Count clips by style
    # 4. Update video document
    video_repo.update_clip_statistics(video_id)


async def transaction_example(
    user_id: str,
    video_id: str,
    clips: list[ClipMetadata],
) -> None:
    """
    Example: Use transactions for atomic operations.
    
    This ensures all clips are created atomically.
    """
    clips_repo = ClipRepository(user_id, video_id)
    
    # Use transaction context manager
    with clips_repo.transaction() as transaction:
        # Create all clips in transaction
        clips_repo.create_clips_batch(clips, transaction=transaction)
        
        # Update video statistics in same transaction
        video_repo = VideoRepository(user_id)
        video_repo.update_clip_statistics(video_id, transaction=transaction)
        
        # Commit transaction (atomic)
        transaction.commit()


def calculate_duration_seconds(start: str, end: str) -> float:
    """Calculate duration in seconds from HH:MM:SS timestamps."""
    def parse_time(time_str: str) -> float:
        parts = time_str.split(":")
        hours = int(parts[0])
        minutes = int(parts[1])
        seconds = float(parts[2])
        return hours * 3600 + minutes * 60 + seconds
    
    return parse_time(end) - parse_time(start)


def generate_presigned_url(r2_key: str) -> Optional[str]:
    """Generate presigned URL from R2 key."""
    from app.core.storage import generate_presigned_url as _generate
    return _generate(r2_key, expires_in=3600)

