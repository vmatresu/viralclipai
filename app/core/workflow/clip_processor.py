"""
Clip processing for video workflow.

Handles clip task creation, rendering, and parallel processing.
"""

import asyncio
import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

from app.config import (
    MAX_CLIP_WORKERS,
    MIN_CLIP_WORKERS,
    MAX_CLIP_WORKERS_LIMIT,
    PROGRESS_HIGHLIGHTS_SAVED,
    PROGRESS_COMPLETE,
)
from app.core import clipper, storage
from app.core.utils import sanitize_filename
from app.core.websocket_messages import send_log, send_progress, send_clip_uploaded
from app.core.workflow.context import ClipTask, ProcessingContext
from app.core.repositories.clips import ClipRepository
from app.core.repositories.models import ClipMetadata

logger = logging.getLogger(__name__)


def create_clip_tasks(
    highlights: List[Dict[str, Any]],
    styles: List[str],
    clips_dir: Path,
    crop_mode: str,
    target_aspect: str,
) -> List[ClipTask]:
    """
    Create clip rendering tasks from highlights and styles.

    Args:
        highlights: List of highlight dictionaries
        styles: List of style strings
        clips_dir: Directory for clip output files
        crop_mode: Crop mode setting
        target_aspect: Target aspect ratio

    Returns:
        List of ClipTask objects
    """
    tasks: List[ClipTask] = []

    for highlight in highlights:
        clip_id = highlight.get("id", 0)
        title = highlight.get("title", f"clip_{clip_id}")
        start = highlight["start"]
        end = highlight["end"]
        pad_before = float(highlight.get("pad_before_seconds", 0) or 0)
        pad_after = float(highlight.get("pad_after_seconds", 0) or 0)
        priority = highlight.get("priority", 99)
        safe_title = sanitize_filename(title)

        for style in styles:
            # Use just priority for ordering (clip_01, clip_02, etc.)
            # clip_id is still tracked internally for scene reference
            filename = f"clip_{priority:02d}_{safe_title}_{style}.mp4"
            out_path = clips_dir / filename

            # Determine effective crop mode
            effective_crop_mode = (
                "intelligent"
                if style in ["intelligent", "intelligent_split"]
                else crop_mode
            )

            # For intelligent_split, ensure 9:16 aspect ratio
            effective_target_aspect = (
                "9:16" if style == "intelligent_split" else target_aspect
            )

            tasks.append(
                ClipTask(
                    title=title,
                    style=style,
                    filename=filename,
                    out_path=out_path,
                    start=start,
                    end=end,
                    effective_crop_mode=effective_crop_mode,
                    effective_target_aspect=effective_target_aspect,
                    pad_before=pad_before,
                    pad_after=pad_after,
                    scene_id=clip_id,
                    scene_description=highlight.get("description"),
                    priority=priority,
                )
            )

    return tasks


def calculate_max_workers() -> int:
    """
    Calculate optimal number of parallel workers for clip processing.

    Returns:
        Number of workers (between MIN_CLIP_WORKERS and MAX_CLIP_WORKERS_LIMIT)
    """
    return max(
        MIN_CLIP_WORKERS,
        min(MAX_CLIP_WORKERS_LIMIT, MAX_CLIP_WORKERS),
    )


def _parse_time(time_str: str) -> float:
    """Parse HH:MM:SS time string to seconds."""
    parts = time_str.split(":")
    hours = int(parts[0])
    minutes = int(parts[1])
    seconds = float(parts[2])
    return hours * 3600 + minutes * 60 + seconds


async def process_clip(
    task: ClipTask,
    video_file: Path,
    shot_cache: Optional[Any],
    context: ProcessingContext,
    semaphore: asyncio.Semaphore,
    completed_lock: asyncio.Lock,
    completed_count: List[int],  # Using list for mutable reference
    total_clips: int,
) -> None:
    """
    Process a single clip: render and upload.

    Args:
        task: ClipTask to process
        video_file: Path to source video file
        shot_cache: Optional shot detection cache
        context: Processing context
        semaphore: Semaphore for concurrency control
        completed_lock: Lock for thread-safe progress updates
        completed_count: List with single int for completed count
        total_clips: Total number of clips being processed
    """
    async with semaphore:
        try:
            logger.info(f"Rendering clip: {task.title} ({task.style})")
            await send_log(
                context.websocket,
                f"✂️ Rendering clip: {task.title} ({task.style})",
            )

            # Render the clip
            await asyncio.to_thread(
                clipper.run_ffmpeg_clip_with_crop,
                task.start,
                task.end,
                task.out_path,
                task.style,
                video_file,
                task.effective_crop_mode,
                task.effective_target_aspect,
                task.pad_before,
                task.pad_after,
                shot_cache,
            )

            # Upload rendered clip and thumbnail to S3
            if context.user_id is not None:
                s3_key = f"{context.user_id}/{context.run_id}/clips/{task.filename}"
                
                # Get file size before upload
                file_size_bytes = task.out_path.stat().st_size if task.out_path.exists() else 0
                thumb_path = task.out_path.with_suffix(".jpg")
                has_thumbnail = thumb_path.exists()
                
                await asyncio.to_thread(
                    storage.upload_file,
                    task.out_path,
                    s3_key,
                    "video/mp4",
                )

                if has_thumbnail:
                    thumb_key = (
                        f"{context.user_id}/{context.run_id}/clips/{thumb_path.name}"
                    )
                    await asyncio.to_thread(
                        storage.upload_file,
                        thumb_path,
                        thumb_key,
                        "image/jpeg",
                    )
                
                # Write clip metadata to Firestore
                try:
                    clips_repo = ClipRepository(context.user_id, context.run_id)
                    clip_id = task.filename.rsplit('.', 1)[0]  # Remove .mp4 extension
                    
                    # Calculate duration in seconds
                    duration_seconds = _parse_time(task.end) - _parse_time(task.start)
                    
                    # Create clip metadata
                    clip_metadata = ClipMetadata(
                        clip_id=clip_id,
                        video_id=context.run_id,
                        user_id=context.user_id,
                        scene_id=task.scene_id,
                        scene_title=task.title,
                        scene_description=task.scene_description,
                        filename=task.filename,
                        style=task.style,
                        priority=task.priority,
                        start_time=task.start,
                        end_time=task.end,
                        duration_seconds=duration_seconds,
                        file_size_bytes=file_size_bytes,
                        has_thumbnail=has_thumbnail,
                        r2_key=s3_key,
                        thumbnail_r2_key=(
                            f"{context.user_id}/{context.run_id}/clips/{thumb_path.name}"
                            if has_thumbnail
                            else None
                        ),
                        status="processing",
                        created_at=datetime.now(timezone.utc),
                        created_by=context.user_id,
                    )
                    
                    # Create in Firestore
                    clips_repo.create_clip(clip_metadata)
                    
                    # Update status to completed after successful upload
                    clips_repo.update_clip_status(
                        clip_id=clip_id,
                        status="completed",
                        file_size_bytes=file_size_bytes,
                        has_thumbnail=has_thumbnail,
                    )
                    
                    logger.debug(f"Created Firestore metadata for clip {clip_id}")
                except Exception as e:
                    # Log error but don't fail the clip processing
                    logger.error(
                        f"Failed to write clip metadata to Firestore for {task.filename}: {e}",
                        exc_info=True,
                    )

            # Update progress and notify frontend (thread-safe)
            async with completed_lock:
                completed_count[0] += 1
                progress = PROGRESS_HIGHLIGHTS_SAVED + int(
                    (completed_count[0] / total_clips) * (PROGRESS_COMPLETE - PROGRESS_HIGHLIGHTS_SAVED)
                )
                await send_progress(context.websocket, progress)
                
                # Send clip_uploaded message for each clip to update frontend cache
                # This allows history page to show clips as they come in
                if context.user_id is not None:
                    await send_clip_uploaded(
                        context.websocket,
                        context.run_id,
                        completed_count[0],
                        total_clips,
                    )
                    
                    # Invalidate backend cache on first clip so history page shows updated data
                    if completed_count[0] == 1:
                        from app.core.cache import get_video_info_cache
                        cache = get_video_info_cache()
                        cache.invalidate(f"{context.user_id}:{context.run_id}")
                        logger.info(f"First clip uploaded for video {context.run_id}, cache invalidated")

        except Exception as e:
            logger.error(
                f"Failed to process clip {task.filename}: {e}",
                exc_info=True,
            )
            # Continue processing other clips even if one fails
            raise


async def process_clips_parallel(
    tasks: List[ClipTask],
    video_file: Path,
    shot_cache: Optional[Any],
    context: ProcessingContext,
) -> None:
    """
    Process all clips in parallel with concurrency control.

    Args:
        tasks: List of ClipTask objects to process
        video_file: Path to source video file
        shot_cache: Optional shot detection cache
        context: Processing context
    """
    if not tasks:
        logger.warning("No clip tasks to process")
        return

    max_workers = calculate_max_workers()
    semaphore = asyncio.Semaphore(max_workers)
    completed_count: List[int] = [0]
    completed_lock = asyncio.Lock()

    logger.info(
        f"Processing {len(tasks)} clips with {max_workers} parallel workers"
    )

    # Process all clips in parallel
    await asyncio.gather(
        *[
            process_clip(
                task,
                video_file,
                shot_cache,
                context,
                semaphore,
                completed_lock,
                completed_count,
                len(tasks),
            )
            for task in tasks
        ],
        return_exceptions=True,  # Continue even if some clips fail
    )
