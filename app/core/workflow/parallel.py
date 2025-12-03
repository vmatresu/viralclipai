"""
Parallel operations for video processing workflow.

Handles concurrent execution of metadata fetch, AI analysis, and video download.
"""

import asyncio
import logging
from pathlib import Path
from typing import Any, Dict, Optional, Tuple

from app.core import clipper
from app.core.gemini import GeminiClient
from app.core.utils import fetch_youtube_title
from app.core.websocket_messages import send_log, send_progress
from app.core.workflow.context import ProcessingContext
from app.config import PROGRESS_PARALLEL_OPS_COMPLETE

logger = logging.getLogger(__name__)


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
