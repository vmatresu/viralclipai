"""
Video processing workflow with parallel execution and robust error handling.

This module implements the main video processing pipeline with:
- Parallel execution of independent operations
- Configurable concurrency limits
- Comprehensive error handling and recovery
- Progress tracking and WebSocket communication
- Resource cleanup and management
"""

import json
import asyncio
import logging
import traceback
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional, Dict, Any, List, Tuple, Union

from fastapi import WebSocket

from app.config import (
    PROMPT_PATH,
    VIDEOS_DIR,
    MAX_CLIP_WORKERS,
    MIN_CLIP_WORKERS,
    MAX_CLIP_WORKERS_LIMIT,
    PROGRESS_INITIAL,
    PROGRESS_PARALLEL_OPS_COMPLETE,
    PROGRESS_HIGHLIGHTS_SAVED,
    PROGRESS_COMPLETE,
    DEFAULT_STYLE,
)
from app.core.utils import (
    extract_youtube_id,
    sanitize_filename,
    generate_run_id,
)
from app.core.workflow.data_processor import (
    normalize_video_title,
    extract_highlights,
    normalize_highlights,
)
from app.core.workflow.validators import (
    resolve_styles,
    validate_plan_limits,
)
from app.core.gemini import GeminiClient
from app.core import clipper
from app.core import saas, storage
from app.core.websocket_messages import (
    send_log,
    send_error,
    send_progress,
    send_done,
    send_clip_uploaded,
)
from app.core.repositories.clips import ClipRepository
from app.core.repositories.videos import VideoRepository
from app.core.repositories.models import ClipMetadata, VideoMetadata

logger = logging.getLogger(__name__)


# ============================================================================
# Data Structures
# ============================================================================


@dataclass
class ClipTask:
    """Represents a single clip rendering task."""

    title: str
    style: str
    filename: str
    out_path: Path
    start: str
    end: str
    effective_crop_mode: str
    effective_target_aspect: str
    pad_before: float
    pad_after: float
    scene_id: int = 0
    scene_description: Optional[str] = None
    priority: int = 99


@dataclass
class ProcessingContext:
    """Context for video processing workflow."""

    websocket: WebSocket
    url: str
    youtube_id: str
    run_id: str
    workdir: Path
    video_file: Path
    clips_dir: Path
    user_id: Optional[str]
    base_prompt: str
    styles: List[str]
    crop_mode: str
    target_aspect: str
    custom_prompt: Optional[str] = None


# ============================================================================
# Prompt Resolution
# ============================================================================


def resolve_prompt(custom_prompt: Optional[str]) -> str:
    """
    Resolve the base prompt using priority order:
    1. User-provided custom prompt
    2. Global admin-configured prompt in Firestore
    3. Local prompt.txt fallback

    Args:
        custom_prompt: Optional user-provided custom prompt

    Returns:
        Resolved base prompt string

    Raises:
        RuntimeError: If no valid prompt can be found
    """
    if custom_prompt and custom_prompt.strip():
        return custom_prompt.strip()

    global_prompt = saas.get_global_prompt()
    if global_prompt:
        return global_prompt

    if not PROMPT_PATH.exists():
        raise RuntimeError(f"prompt.txt not found at {PROMPT_PATH}")

    return PROMPT_PATH.read_text(encoding="utf-8")


# ============================================================================
# Parallel Operations
# ============================================================================


async def fetch_video_metadata(url: str) -> Optional[str]:
    """
    Fetch video metadata (title) from YouTube.

    Args:
        url: YouTube video URL

    Returns:
        Video title or None if fetch fails
    """
    try:
        return await asyncio.to_thread(fetch_youtube_title, url)
    except Exception as e:
        logger.warning(f"Failed to fetch video metadata: {e}")
        return None


async def run_ai_analysis(
    base_prompt: str,
    url: str,
    workdir: Path,
) -> Dict[str, Any]:
    """
    Run AI analysis to extract highlights from video.

    Args:
        base_prompt: Base prompt for AI analysis
        url: YouTube video URL
        workdir: Working directory for analysis artifacts

    Returns:
        Analysis data dictionary with highlights

    Raises:
        RuntimeError: If AI analysis fails
    """
    try:
        client = GeminiClient()
        return await asyncio.to_thread(
            client.get_highlights,
            base_prompt,
            url,
            workdir,
        )
    except Exception as e:
        logger.error(f"AI analysis failed: {e}")
        raise RuntimeError(f"AI analysis failed: {e}") from e


async def download_video(url: str, video_file: Path) -> Path:
    """
    Download video from URL.

    Args:
        url: Video URL
        video_file: Path where video will be saved

    Returns:
        Path to downloaded video file

    Raises:
        RuntimeError: If download fails
    """
    try:
        await asyncio.to_thread(clipper.download_video, url, video_file)
        return video_file
    except Exception as e:
        logger.error(f"Video download failed: {e}")
        raise RuntimeError(f"Video download failed: {e}") from e


async def execute_parallel_operations(
    context: ProcessingContext,
) -> Tuple[Optional[str], Dict[str, Any], Path]:
    """
    Execute metadata fetch, AI analysis, and video download in parallel.

    Args:
        context: Processing context

    Returns:
        Tuple of (video_title, analysis_data, video_file_path)

    Raises:
        RuntimeError: If critical operations fail
    """
    logger.info(
        f"Starting parallel operations for video {context.youtube_id}: "
        "metadata fetch, AI analysis, and video download"
    )
    await send_log(context.websocket, "📋 Fetching video metadata...")
    await send_log(context.websocket, "🤖 Analyzing video with AI...")
    await send_log(context.websocket, "📥 Downloading video...")

    # Execute all three operations in parallel
    # Use return_exceptions=True to handle partial failures gracefully
    results = await asyncio.gather(
        fetch_video_metadata(context.url),
        run_ai_analysis(context.base_prompt, context.url, context.workdir),
        download_video(context.url, context.video_file),
        return_exceptions=True,
    )

    video_title, analysis_data, video_file = results

    # Handle exceptions
    if isinstance(analysis_data, Exception):
        raise RuntimeError(
            f"AI analysis failed: {analysis_data}"
        ) from analysis_data

    if isinstance(video_file, Exception):
        raise RuntimeError(
            f"Video download failed: {video_file}"
        ) from video_file

    # Metadata fetch failure is non-critical
    if isinstance(video_title, Exception):
        logger.warning(f"Metadata fetch failed: {video_title}")
        video_title = None

    logger.info("✅ Parallel operations complete.")
    await send_log(context.websocket, "✅ AI analysis complete.")
    await send_log(context.websocket, "✅ Download complete.")
    await send_progress(context.websocket, PROGRESS_PARALLEL_OPS_COMPLETE)

    return video_title, analysis_data, video_file


# ============================================================================
# Data Processing & Validation
# ============================================================================
# Functions moved to separate modules for better organization
# Imported at top of file


# ============================================================================
# Clip Processing
# ============================================================================


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
            filename = f"clip_{priority:02d}_{clip_id:02d}_{safe_title}_{style}.mp4"
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
                    def parse_time(time_str: str) -> float:
                        parts = time_str.split(":")
                        hours = int(parts[0])
                        minutes = int(parts[1])
                        seconds = float(parts[2])
                        return hours * 3600 + minutes * 60 + seconds
                    
                    duration_seconds = parse_time(task.end) - parse_time(task.start)
                    
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


# ============================================================================
# Main Workflow
# ============================================================================


async def process_video_workflow(
    websocket: WebSocket,
    url: str,
    styles: List[str],
    user_id: Optional[str] = None,
    custom_prompt: Optional[str] = None,
    crop_mode: str = "none",
    target_aspect: str = "9:16",
) -> None:
    """
    Main video processing workflow with parallel execution.

    This function orchestrates the entire video processing pipeline:
    1. Resolve prompt and initialize context
    2. Execute parallel operations (metadata, AI analysis, download)
    3. Process highlights and validate plan limits
    4. Render clips in parallel
    5. Cleanup and record usage

    Args:
        websocket: WebSocket connection for progress updates
        url: YouTube video URL
        styles: List of style strings to process
        user_id: Optional user ID for authentication
        custom_prompt: Optional custom prompt override
        crop_mode: Crop mode setting
        target_aspect: Target aspect ratio
    """
    context: Optional[ProcessingContext] = None

    try:
        # Initialize context
        base_prompt = resolve_prompt(custom_prompt)
        youtube_id = extract_youtube_id(url)
        run_id = generate_run_id()
        workdir = VIDEOS_DIR / run_id
        workdir.mkdir(parents=True, exist_ok=True)

        video_file = workdir / "source.mp4"
        clips_dir = clipper.ensure_dirs(workdir)

        context = ProcessingContext(
            websocket=websocket,
            url=url,
            youtube_id=youtube_id,
            run_id=run_id,
            workdir=workdir,
            video_file=video_file,
            clips_dir=clips_dir,
            user_id=user_id,
            base_prompt=base_prompt,
            styles=styles,
            crop_mode=crop_mode,
            target_aspect=target_aspect,
            custom_prompt=custom_prompt,
        )

        logger.info(
            f"Starting job for Video ID: {youtube_id} in {workdir}"
        )
        await send_log(websocket, f"🚀 Starting job for Video ID: {youtube_id}")
        await send_progress(websocket, PROGRESS_INITIAL)

        # Execute parallel operations
        video_title, analysis_data, _ = await execute_parallel_operations(context)

        # Normalize and process data
        analysis_data["video_title"] = normalize_video_title(
            analysis_data,
            video_title,
            youtube_id,
        )
        analysis_data["video_url"] = url
        if custom_prompt and custom_prompt.strip():
            analysis_data["custom_prompt"] = custom_prompt.strip()

        # Extract and normalize highlights
        highlights = extract_highlights(analysis_data)
        normalize_highlights(highlights)

        # Resolve styles
        styles_to_process = resolve_styles(styles)
        total_clips = len(highlights) * len(styles_to_process)

        # Validate plan limits
        try:
            validate_plan_limits(user_id, total_clips)
        except ValueError as e:
            await send_error(websocket, str(e))
            return

        # Save highlights
        highlights_path = workdir / "highlights.json"
        with open(highlights_path, "w", encoding="utf-8") as f:
            json.dump(analysis_data, f, indent=2, ensure_ascii=False)

        # Upload highlights to S3
        if user_id is not None:
            await asyncio.to_thread(
                storage.upload_file,
                highlights_path,
                f"{user_id}/{run_id}/highlights.json",
                "application/json",
            )
            
            # Create history entry early with "processing" status
            # This allows users to see the video in history while processing
            final_title = (
                analysis_data.get("video_title")
                or video_title
                or f"Video {youtube_id}"
            )
            saas.record_video_job(
                user_id,
                run_id,
                url,
                final_title,
                total_clips,
                custom_prompt=custom_prompt.strip() if custom_prompt else None,
                status="processing",
            )
            logger.info(f"Created history entry for video {run_id} with processing status")
            
            # Create video metadata in Firestore
            try:
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
                
                # Create video metadata
                video_metadata = VideoMetadata(
                    video_id=run_id,
                    user_id=user_id,
                    video_url=url,
                    video_title=final_title,
                    youtube_id=youtube_id,
                    status="processing",
                    created_at=datetime.now(timezone.utc),
                    updated_at=datetime.now(timezone.utc),
                    highlights_count=len(highlights),
                    highlights_summary=highlights_summary,
                    custom_prompt=custom_prompt.strip() if custom_prompt else None,
                    styles_processed=styles_to_process,
                    crop_mode=crop_mode,
                    target_aspect=target_aspect,
                    clips_count=0,  # Will be updated as clips are created
                    clips_by_style={},  # Will be updated as clips are created
                    highlights_json_key=f"{user_id}/{run_id}/highlights.json",
                    created_by=user_id,
                )
                
                # Create in Firestore
                video_repo.create_or_update_video(video_metadata)
                logger.info(f"Created Firestore metadata for video {run_id}")
            except Exception as e:
                # Log error but don't fail the workflow
                logger.error(
                    f"Failed to write video metadata to Firestore for {run_id}: {e}",
                    exc_info=True,
                )

        await send_progress(websocket, PROGRESS_HIGHLIGHTS_SAVED)

        # Initialize shot detection cache for intelligent cropping
        from app.core.smart_reframe.cache import get_shot_cache

        shot_cache = (
            get_shot_cache() if crop_mode == "intelligent" else None
        )

        # Create and process clip tasks
        clip_tasks = create_clip_tasks(
            highlights,
            styles_to_process,
            clips_dir,
            crop_mode,
            target_aspect,
        )

        await process_clips_parallel(
            clip_tasks,
            video_file,
            shot_cache,
            context,
        )

        # Cleanup
        if video_file.exists():
            video_file.unlink()
            logger.info("Cleaned up source video file.")
            await send_log(websocket, "🧹 Cleaned up source video file.")

        logger.info("Job complete.")

        # Update video status to completed and invalidate cache
        if user_id is not None and total_clips > 0:
            final_title = (
                analysis_data.get("video_title")
                or video_title
                or f"Video {youtube_id}"
            )
            # Update status to completed (history entry was already created earlier)
            saas.update_video_status(
                user_id,
                run_id,
                "completed",
                clips_count=total_clips,
            )
            
            # Update video status in Firestore and update statistics
            try:
                video_repo = VideoRepository(user_id)
                clips_repo = ClipRepository(user_id, run_id)
                
                # Update clip statistics
                video_repo.update_clip_statistics(run_id)
                
                # Update video status to completed
                video_repo.update_video_status(
                    video_id=run_id,
                    status="completed",
                )
                logger.info(f"Updated Firestore video {run_id} status to completed")
            except Exception as e:
                # Log error but don't fail the workflow
                logger.error(
                    f"Failed to update Firestore video status for {run_id}: {e}",
                    exc_info=True,
                )
            
            # Invalidate backend cache so history page shows completed status
            from app.core.cache import get_video_info_cache
            cache = get_video_info_cache()
            cache.invalidate(f"{user_id}:{run_id}")
            logger.info(f"Updated video {run_id} status to completed and invalidated cache")

        await send_progress(websocket, PROGRESS_COMPLETE)
        await send_log(websocket, "✨ All done!")
        await send_done(websocket, run_id)

    except ValueError as e:
        # User-facing validation errors
        logger.warning(f"Validation error: {e}")
        await send_error(websocket, str(e))
    except Exception as e:
        # Unexpected errors
        error_trace = traceback.format_exc()
        logger.error(f"Error processing video: {e}\n{error_trace}")
        await send_error(websocket, str(e), details=error_trace)
    finally:
        # Ensure cleanup even on error
        if context and context.video_file.exists():
            try:
                context.video_file.unlink()
                logger.info("Cleaned up source video file after error.")
            except Exception as e:
                logger.warning(f"Failed to cleanup video file: {e}")


# Reprocessing workflow moved to app.core.reprocessing module
# Import here for backward compatibility
from app.core.reprocessing import reprocess_scenes_workflow

__all__ = ["reprocess_scenes_workflow"]
