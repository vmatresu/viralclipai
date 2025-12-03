"""
Data structures for video processing workflow.

Contains dataclasses for processing context and clip tasks.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

from fastapi import WebSocket


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
    
    # Fields for segment-based processing
    source_path: Optional[Path] = None
    processing_start: Optional[str] = None
    processing_end: Optional[str] = None
    processing_pad_before: Optional[float] = None
    processing_pad_after: Optional[float] = None


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
