"""
Main video processing workflow.

Orchestrates the entire video processing pipeline with parallel execution,
progress tracking, and resource management.
"""

import json
import asyncio
import logging
import traceback
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Optional

from fastapi import WebSocket

from app.config import (
    VIDEOS_DIR,
    PROGRESS_INITIAL,
    PROGRESS_HIGHLIGHTS_SAVED,
    PROGRESS_COMPLETE,
)
from app.core import clipper, saas, storage
from app.core.utils import extract_youtube_id, generate_run_id
from app.core.websocket_messages import (
    send_log,
    send_error,
    send_progress,
    send_done,
)
from app.core.repositories.clips import ClipRepository
from app.core.repositories.videos import VideoRepository
from app.core.repositories.models import VideoMetadata

from app.core.workflow.context import ProcessingContext
from app.core.workflow.prompt import resolve_prompt
from app.core.workflow.data_processor import (
    normalize_video_title,
    extract_highlights,
    normalize_highlights,
)
from app.core.workflow.validators import resolve_styles, validate_plan_limits
from app.core.workflow.parallel import execute_parallel_operations, download_video
from app.core.workflow.clip_processor import create_clip_tasks, process_clips_parallel

logger = logging.getLogger(__name__)


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
            
            # Invalidate backend caches so history page shows completed status
            from app.core.cache import get_video_info_cache, get_user_videos_cache
            cache = get_video_info_cache()
            cache.invalidate(f"{user_id}:{run_id}")
            user_videos_cache = get_user_videos_cache()
            user_videos_cache.invalidate(f"user_videos:{user_id}")
            logger.info(f"Updated video {run_id} status to completed and invalidated caches")

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
        # Ensure video status is reset if processing failed
        if user_id is not None and run_id:
            try:
                # Check if we're still in processing state (indicates error)
                current_status = saas.is_video_processing(user_id, run_id)
                if current_status:
                    # Reset status to completed on error to prevent stuck processing state
                    saas.update_video_status(user_id, run_id, "completed")
                    logger.info(f"Reset video {run_id} status to completed after processing error")

                    # Invalidate caches so history page shows correct status
                    from app.core.cache import get_video_info_cache, get_user_videos_cache
                    cache = get_video_info_cache()
                    cache.invalidate(f"{user_id}:{run_id}")
                    user_videos_cache = get_user_videos_cache()
                    user_videos_cache.invalidate(f"user_videos:{user_id}")
            except Exception as cleanup_error:
                logger.error(
                    f"Error during processing cleanup for {run_id}: {cleanup_error}",
                    exc_info=True,
                )

        # Ensure cleanup even on error
        if context and context.video_file.exists():
            try:
                context.video_file.unlink()
                logger.info("Cleaned up source video file after error.")
            except Exception as e:
                logger.warning(f"Failed to cleanup video file: {e}")
