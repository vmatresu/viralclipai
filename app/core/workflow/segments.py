"""
Segment management service.

Handles the extraction, management, and cleanup of video segments.
Follows the Single Responsibility Principle for segment operations.
"""

import asyncio
import logging
import shutil
from pathlib import Path
from typing import Dict, List, Any, Optional

from app.core import clipper
from app.core.websocket_messages import send_log
from fastapi import WebSocket

logger = logging.getLogger(__name__)

class SegmentManager:
    """
    Manages the lifecycle of video segments.
    
    This class handles the parallel extraction of high-quality intermediate
    segments from a source video and manages their storage and cleanup.
    """
    
    def __init__(self, workdir: Path, max_workers: int = 4):
        """
        Initialize the SegmentManager.
        
        Args:
            workdir: Base working directory for the current job.
            max_workers: Maximum number of concurrent extraction processes.
        """
        self.workdir = workdir
        self.segments_dir = workdir / "segments"
        self.max_workers = max_workers
        self._segment_map: Dict[int, Path] = {}
        
        # Ensure directory exists
        self.segments_dir.mkdir(parents=True, exist_ok=True)

    async def extract_segments(
        self, 
        highlights: List[Dict[str, Any]], 
        video_path: Path,
        websocket: Optional[WebSocket] = None
    ) -> Dict[int, Path]:
        """
        Extract segments for all highlights in parallel.
        
        Args:
            highlights: List of highlight dictionaries.
            video_path: Path to the source video.
            websocket: Optional WebSocket for logging.
            
        Returns:
            Dictionary mapping scene ID to segment file path.
        """
        self._segment_map = {}
        semaphore = asyncio.Semaphore(self.max_workers)
        
        if websocket:
            await send_log(websocket, f"✂️ Extracting {len(highlights)} segments in parallel...")
            
        async def _extract_single(highlight: Dict[str, Any]):
            async with semaphore:
                clip_id = highlight.get("id", 0)
                start = highlight["start"]
                end = highlight["end"]
                pad_before = float(highlight.get("pad_before_seconds", 0) or 0)
                pad_after = float(highlight.get("pad_after_seconds", 0) or 0)
                
                segment_filename = f"segment_{clip_id}.mp4"
                segment_path = self.segments_dir / segment_filename
                
                try:
                    # Use asyncio.to_thread to avoid blocking the event loop
                    await asyncio.to_thread(
                        clipper.extract_segment,
                        video_path,
                        start,
                        end,
                        segment_path,
                        pad_before,
                        pad_after,
                    )
                    return clip_id, segment_path
                except Exception as e:
                    logger.error(f"Failed to extract segment {clip_id}: {e}")
                    if websocket:
                        await send_log(websocket, f"⚠️ Failed to extract segment {clip_id}: {e}")
                    return None

        # Execute all extractions concurrently
        tasks = [_extract_single(h) for h in highlights]
        results = await asyncio.gather(*tasks)
        
        # Collect successful results
        for result in results:
            if result:
                clip_id, path = result
                self._segment_map[clip_id] = path
                
        logger.info(f"Extracted {len(self._segment_map)} segments")
        return self._segment_map

    def get_segment_map(self) -> Dict[int, Path]:
        """Get the current segment map."""
        return self._segment_map

    def cleanup(self):
        """Remove the segments directory and all contents."""
        if self.segments_dir.exists():
            try:
                shutil.rmtree(self.segments_dir, ignore_errors=True)
                logger.debug(f"Cleaned up segments directory: {self.segments_dir}")
            except Exception as e:
                logger.warning(f"Failed to cleanup segments directory: {e}")
