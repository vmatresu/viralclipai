"""
OpenCV utilities with FFmpeg warning suppression.

This module provides wrappers for OpenCV operations that suppress
AV1 and other benign FFmpeg warnings from appearing in logs.
"""

import contextlib
import logging
import os
import sys
from typing import Optional

import cv2

logger = logging.getLogger(__name__)


@contextlib.contextmanager
def suppress_ffmpeg_warnings():
    """
    Context manager to suppress FFmpeg warnings from OpenCV operations.
    
    Temporarily redirects stderr to filter out benign AV1 warnings
    while preserving actual errors.
    """
    import io
    from app.core.utils.ffmpeg import filter_benign_warnings
    
    # Save original stderr
    original_stderr = sys.stderr
    
    # Create a StringIO to capture stderr
    stderr_capture = io.StringIO()
    
    try:
        # Redirect stderr
        sys.stderr = stderr_capture
        yield
    finally:
        # Restore original stderr
        sys.stderr = original_stderr
        
        # Get captured output and filter warnings
        captured = stderr_capture.getvalue()
        if captured:
            filtered, warnings = filter_benign_warnings(captured)
            if warnings:
                logger.debug(f"Suppressed {len(warnings)} FFmpeg warnings from OpenCV")
            # Only log non-benign output
            if filtered.strip():
                # Write filtered output to stderr (for actual errors)
                original_stderr.write(filtered)


def VideoCapture_safe(video_path: str) -> cv2.VideoCapture:
    """
    Open a video file with suppressed FFmpeg warnings.
    
    Args:
        video_path: Path to video file.
        
    Returns:
        cv2.VideoCapture object.
    """
    with suppress_ffmpeg_warnings():
        cap = cv2.VideoCapture(video_path)
    return cap

