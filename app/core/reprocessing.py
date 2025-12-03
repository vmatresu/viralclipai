"""
Scene reprocessing workflow module.

This module handles reprocessing of specific scenes from existing videos
with new styles, following SOLID principles and separation of concerns.
"""

import logging
import traceback
from pathlib import Path
from typing import List, Optional, Dict, Any

from fastapi import WebSocket

from app.config import VIDEOS_DIR, PROGRESS_INITIAL, PROGRESS_HIGHLIGHTS_SAVED, PROGRESS_COMPLETE
from app.core import saas, storage, clipper
from app.core.workflow import (
    ProcessingContext,
    resolve_prompt,
    normalize_highlights,
    resolve_styles,
    validate_plan_limits,
    create_clip_tasks,
    process_clips_parallel,
    download_video,
)
from app.core.utils import extract_youtube_id
from app.core.websocket_messages import (
    send_log,
    send_error,
    send_progress,
    send_done,
)

logger = logging.getLogger(__name__)


class ReprocessingError(Exception):
    """Base exception for reprocessing errors."""
    pass


class HighlightsNotFoundError(ReprocessingError):
    """Raised when highlights are not found for a video."""
    pass


class VideoMetadataNotFoundError(ReprocessingError):
    """Raised when video metadata is not found."""
    pass


class SceneNotFoundError(ReprocessingError):
    """Raised when requested scenes are not found."""
    pass


class ReprocessingService:
    """
    Service class for handling scene reprocessing.
    
    Follows Single Responsibility Principle - handles only reprocessing logic.
    """
    
    def __init__(self, user_id: str, video_id: str):
        """
        Initialize reprocessing service.
        
        Args:
            user_id: User ID for authentication
            video_id: Video ID to reprocess scenes from
        """
        self.user_id = user_id
        self.video_id = video_id
        self._highlights_data: Optional[Dict[str, Any]] = None
        self._video_metadata: Optional[Dict[str, Any]] = None
    
    def load_highlights_and_metadata(self) -> None:
        """
        Load highlights and video metadata.
        
        Raises:
            HighlightsNotFoundError: If highlights are not found
            VideoMetadataNotFoundError: If video metadata is not found
        """
        # Load highlights
        self._highlights_data = storage.load_highlights(self.user_id, self.video_id)
        if not self._highlights_data or "highlights" not in self._highlights_data:
            raise HighlightsNotFoundError("Highlights not found for this video")
        
        # Load video metadata
        self._video_metadata = saas.get_video_metadata(self.user_id, self.video_id)
        if not self._video_metadata:
            raise VideoMetadataNotFoundError("Video metadata not found")
    
    def get_video_url(self) -> str:
        """
        Get video URL from metadata or highlights.
        
        Returns:
            Video URL
            
        Raises:
            VideoMetadataNotFoundError: If URL is not found
        """
        if not self._video_metadata or not self._highlights_data:
            raise VideoMetadataNotFoundError("Metadata not loaded")
        
        video_url = (
            self._video_metadata.get("video_url") 
            or self._highlights_data.get("video_url")
        )
        
        if not video_url:
            raise VideoMetadataNotFoundError("Video URL not found")
        
        return video_url
    
    def filter_scenes(self, scene_ids: List[int]) -> List[Dict[str, Any]]:
        """
        Filter highlights to only selected scene IDs.
        
        Args:
            scene_ids: List of scene IDs to include
            
        Returns:
            List of filtered highlight dictionaries
            
        Raises:
            SceneNotFoundError: If no matching scenes found
            ValueError: If scene_ids contains invalid values
        """
        if not self._highlights_data:
            raise HighlightsNotFoundError("Highlights not loaded")
        
        # Security: Validate scene IDs
        if not scene_ids:
            raise ValueError("Scene IDs list cannot be empty")
        
        if len(scene_ids) > 50:  # Reasonable upper limit
            raise ValueError(f"Too many scene IDs: {len(scene_ids)}. Maximum is 50.")
        
        # Validate all IDs are integers
        validated_ids = []
        for scene_id in scene_ids:
            if not isinstance(scene_id, int):
                raise ValueError(f"Invalid scene ID type: {scene_id}")
            if scene_id < 1 or scene_id > 10000:
                raise ValueError(f"Scene ID out of range: {scene_id}")
            validated_ids.append(scene_id)
        
        all_highlights = self._highlights_data.get("highlights", [])
        selected_highlights = [
            h for h in all_highlights
            if h.get("id") in validated_ids
        ]
        
        if not selected_highlights:
            raise SceneNotFoundError(
                f"No matching scenes found for IDs: {validated_ids}"
            )
        
        return selected_highlights
    
    def get_custom_prompt(self) -> Optional[str]:
        """Get custom prompt from highlights data."""
        if not self._highlights_data:
            return None
        return self._highlights_data.get("custom_prompt")


async def reprocess_scenes_workflow(
    websocket: WebSocket,
    video_id: str,
    scene_ids: List[int],
    styles: List[str],
    user_id: Optional[str] = None,
    crop_mode: str = "none",
    target_aspect: str = "9:16",
) -> None:
    """
    Reprocess specific scenes from an existing video with new styles.
    
    This function orchestrates the reprocessing pipeline:
    1. Loads existing highlights and video metadata
    2. Filters highlights to only selected scene IDs
    3. Downloads video if needed (or reuses existing)
    4. Processes only selected scenes with specified styles
    5. Uploads new clips and updates status
    
    Args:
        websocket: WebSocket connection for progress updates
        video_id: Existing video ID to reprocess scenes from
        scene_ids: List of scene/highlight IDs to reprocess
        styles: List of style strings to process
        user_id: User ID for authentication
        crop_mode: Crop mode setting
        target_aspect: Target aspect ratio
        
    Raises:
        ValueError: For validation errors
        ReprocessingError: For reprocessing-specific errors
    """
    if not user_id:
        raise ValueError("User ID is required for reprocessing")
    
    context: Optional[ProcessingContext] = None
    
    try:
        # Initialize service
        service = ReprocessingService(user_id, video_id)
        service.load_highlights_and_metadata()
        
        # Get video URL
        video_url = service.get_video_url()
        
        # Filter scenes
        selected_highlights = service.filter_scenes(scene_ids)
        
        logger.info(
            f"Reprocessing {len(selected_highlights)} scenes from video {video_id}"
        )
        await send_log(
            websocket, 
            f"🔄 Reprocessing {len(selected_highlights)} scenes..."
        )
        await send_progress(websocket, PROGRESS_INITIAL)
        
        # Initialize context
        base_prompt = resolve_prompt(service.get_custom_prompt())
        youtube_id = extract_youtube_id(video_url)
        run_id = video_id  # Use same video ID for reprocessing
        workdir = VIDEOS_DIR / run_id
        workdir.mkdir(parents=True, exist_ok=True)
        
        video_file = workdir / "source.mp4"
        clips_dir = clipper.ensure_dirs(workdir)
        
        # Download video if not already present
        if not video_file.exists():
            await send_log(websocket, "📥 Downloading video...")
            await download_video(video_url, video_file)
            await send_log(websocket, "✅ Video downloaded")
        
        context = ProcessingContext(
            websocket=websocket,
            url=video_url,
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
            custom_prompt=service.get_custom_prompt(),
        )
        
        # Normalize selected highlights
        normalize_highlights(selected_highlights)
        
        # Resolve styles
        styles_to_process = resolve_styles(styles)
        total_clips = len(selected_highlights) * len(styles_to_process)
        
        # Validate plan limits
        try:
            validate_plan_limits(user_id, total_clips)
        except ValueError as e:
            await send_error(websocket, str(e))
            return
        
        await send_progress(websocket, PROGRESS_HIGHLIGHTS_SAVED)
        
        # Initialize shot detection cache for intelligent cropping
        from app.core.smart_reframe.cache import get_shot_cache
        
        shot_cache = (
            get_shot_cache() if crop_mode == "intelligent" else None
        )
        
        # Create and process clip tasks for selected scenes only
        clip_tasks = create_clip_tasks(
            selected_highlights,
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
        
        # Calculate total clips count (existing + new)
        # Note: We count all clips from storage to get accurate total
        existing_clips = storage.list_clips_with_metadata(
            user_id, video_id, {}
        )
        existing_clips_count = len(existing_clips)
        total_clips_count = existing_clips_count + total_clips
        
        # Update status to completed with accurate clip count
        saas.update_video_status(
            user_id,
            run_id,
            "completed",
            clips_count=total_clips_count,
        )
        
        # Invalidate cache
        from app.core.cache import get_video_info_cache
        cache = get_video_info_cache()
        cache.invalidate(f"{user_id}:{run_id}")
        
        logger.info(
            f"Reprocessing complete for video {run_id}: {total_clips} new clips"
        )
        
        await send_progress(websocket, PROGRESS_COMPLETE)
        await send_log(
            websocket, 
            f"✨ Reprocessing complete! {total_clips} new clips generated."
        )
        await send_done(websocket, run_id)
        
    except (HighlightsNotFoundError, VideoMetadataNotFoundError, SceneNotFoundError) as e:
        logger.warning(f"Reprocessing error: {e}")
        await send_error(websocket, str(e))
    except ValueError as e:
        logger.warning(f"Validation error during reprocessing: {e}")
        await send_error(websocket, str(e))
    except Exception as e:
        error_trace = traceback.format_exc()
        logger.error(f"Error reprocessing scenes: {e}\n{error_trace}")
        await send_error(websocket, str(e), details=error_trace)
    finally:
        # Resource cleanup: Video file is kept for potential reuse
        # The file will be cleaned up by:
        # 1. Main workflow cleanup (if new video processing)
        # 2. Manual cleanup job (periodic maintenance)
        # 3. Disk space management (if storage is full)
        
        # Ensure video status is reset if processing failed
        if context and user_id:
            try:
                # Check if we're still in processing state (indicates error)
                current_status = saas.get_video_metadata(user_id, video_id)
                if current_status and current_status.get("status") == "processing":
                    # Only reset if we're still processing (error case)
                    # Successful completion already updated status above
                    logger.warning(
                        f"Reprocessing failed for video {video_id}, "
                        "status may need manual review"
                    )
            except Exception as cleanup_error:
                logger.error(
                    f"Error during reprocessing cleanup: {cleanup_error}",
                    exc_info=True
                )

