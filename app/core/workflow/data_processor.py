"""
Data processing utilities for video workflow.

Handles normalization, extraction, and transformation of video data
following Single Responsibility Principle.
"""

import logging
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)


def normalize_video_title(
    data: Dict[str, Any],
    fetched_title: Optional[str],
    youtube_id: str,
) -> str:
    """
    Normalize video title from multiple sources.

    Args:
        data: Analysis data dictionary
        fetched_title: Title fetched from YouTube API
        youtube_id: YouTube video ID

    Returns:
        Normalized video title
    """
    if fetched_title:
        return fetched_title

    title = data.get("video_title")
    if title and title != "The Main Title of the YouTube Video":
        return title

    return f"Video {youtube_id}"


def extract_highlights(data: Dict[str, Any]) -> List[Dict[str, Any]]:
    """
    Extract highlights list from analysis data.

    Args:
        data: Analysis data dictionary

    Returns:
        List of highlight dictionaries

    Raises:
        ValueError: If no valid highlights found
    """
    # Check standard key first
    if "highlights" in data and isinstance(data["highlights"], list):
        return data["highlights"]

    # Fallback: Look for any list that looks like highlight data
    for key, value in data.items():
        if isinstance(value, list) and len(value) > 0:
            first_item = value[0]
            if isinstance(first_item, dict) and "start" in first_item and "end" in first_item:
                logger.info(f"Found alternate highlights key: {key}")
                return value

    raise ValueError("No valid highlights found in AI response")


def normalize_highlights(highlights: List[Dict[str, Any]]) -> None:
    """
    Normalize highlight entries with default values.

    Args:
        highlights: List of highlight dictionaries (modified in-place)
    """
    for idx, highlight in enumerate(highlights, start=1):
        highlight.setdefault("id", idx)
        highlight.setdefault("priority", idx)
        highlight.setdefault("title", f"Clip {idx}")
        
        # Ensure pad_before_seconds and pad_after_seconds exist
        highlight.setdefault("pad_before_seconds", 0.0)
        highlight.setdefault("pad_after_seconds", 0.0)
        
        # Ensure duration is calculated if not present
        if "duration" not in highlight or highlight["duration"] == 0:
            try:
                start = highlight.get("start", "")
                end = highlight.get("end", "")
                if start and end:
                    # Parse HH:MM:SS format
                    def parse_time(time_str: str) -> float:
                        parts = time_str.split(":")
                        if len(parts) == 3:
                            return (
                                float(parts[0]) * 3600
                                + float(parts[1]) * 60
                                + float(parts[2])
                            )
                        return 0.0
                    
                    duration = parse_time(end) - parse_time(start)
                    highlight["duration"] = int(duration)
            except (ValueError, KeyError):
                highlight["duration"] = 0

