"""
WebSocket Message Utilities

Centralized WebSocket message sending with proper error handling
and type safety.
"""

import logging
from datetime import datetime, timezone
from typing import Optional

from fastapi import WebSocket

logger = logging.getLogger(__name__)


# WebSocket message types
WS_MSG_TYPE_LOG = "log"
WS_MSG_TYPE_PROGRESS = "progress"
WS_MSG_TYPE_ERROR = "error"
WS_MSG_TYPE_DONE = "done"
WS_MSG_TYPE_CLIP_UPLOADED = "clip_uploaded"


async def send_log(websocket: WebSocket, message: str) -> None:
    """
    Send a timestamped log message via WebSocket.

    Args:
        websocket: WebSocket connection
        message: Log message to send
    """
    timestamp = datetime.now(timezone.utc).isoformat()
    try:
        await websocket.send_json({
            "type": WS_MSG_TYPE_LOG,
            "message": message,
            "timestamp": timestamp,
        })
    except Exception as e:
        logger.warning(f"Failed to send log message via WebSocket: {e}")


async def send_error(
    websocket: WebSocket,
    message: str,
    details: Optional[str] = None,
) -> None:
    """
    Send an error message via WebSocket.

    Args:
        websocket: WebSocket connection
        message: Error message
        details: Optional error details/traceback
    """
    timestamp = datetime.now(timezone.utc).isoformat()
    try:
        await websocket.send_json({
            "type": WS_MSG_TYPE_ERROR,
            "message": message,
            "details": details,
            "timestamp": timestamp,
        })
    except Exception as e:
        logger.warning(f"Failed to send error message via WebSocket: {e}")


async def send_progress(websocket: WebSocket, value: int) -> None:
    """
    Send progress update via WebSocket.

    Args:
        websocket: WebSocket connection
        value: Progress value (0-100)
    """
    try:
        await websocket.send_json({
            "type": WS_MSG_TYPE_PROGRESS,
            "value": value,
        })
    except Exception as e:
        logger.warning(f"Failed to send progress update via WebSocket: {e}")


async def send_done(websocket: WebSocket, video_id: str) -> None:
    """
    Send processing completion message via WebSocket.

    Args:
        websocket: WebSocket connection
        video_id: Video ID that completed processing
    """
    try:
        await websocket.send_json({
            "type": WS_MSG_TYPE_DONE,
            "videoId": video_id,
        })
    except Exception as e:
        logger.warning(f"Failed to send done message via WebSocket: {e}")


async def send_clip_uploaded(
    websocket: WebSocket,
    video_id: str,
    clip_count: int,
    total_clips: int,
) -> None:
    """
    Send clip upload notification via WebSocket.

    Args:
        websocket: WebSocket connection
        video_id: Video ID
        clip_count: Number of clips uploaded so far
        total_clips: Total number of clips expected
    """
    try:
        await websocket.send_json({
            "type": WS_MSG_TYPE_CLIP_UPLOADED,
            "videoId": video_id,
            "clipCount": clip_count,
            "totalClips": total_clips,
        })
    except Exception as e:
        logger.warning(f"Failed to send clip_uploaded message via WebSocket: {e}")

