from typing import Dict, Any, Optional

from fastapi import APIRouter, Depends, HTTPException, status, Response, Request
from fastapi.responses import StreamingResponse

from app.core.firebase_client import get_current_user
from app.core import saas, storage
from app.core.security import ValidationError, validate_video_id
from app.schemas import VideoInfoResponse, UserVideosResponse

router = APIRouter(prefix="/api", tags=["Videos"])

@router.get("/videos/{video_id}", response_model=VideoInfoResponse)
async def get_video_info(
    video_id: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> VideoInfoResponse:
    """Get video information and clips."""
    # Validate video_id to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    uid = user["uid"]
    from app.config import logger

    # Check ownership in DB first
    is_owner = saas.user_owns_video(uid, video_id)
    
    # Load highlights metadata (from storage)
    highlights_data = storage.load_highlights(uid, video_id)
    
    logger.info(f"Debug get_video_info: uid={uid} video_id={video_id} is_owner={is_owner} highlights_found={bool(highlights_data)}")

    # If not in DB and not in storage, then it truly doesn't exist (or isn't yours)
    if not is_owner and not highlights_data:
        logger.warning(f"Video not found for user {uid}: {video_id}")
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    highlights_map: Dict[int, Dict[str, str]] = {}
    for h in highlights_data.get("highlights", []):
        h_id = h.get("id")
        if h_id is not None:
            highlights_map[int(h_id)] = {
                "title": h.get("title", ""),
                "description": h.get("description", ""),
            }
    
    clips = storage.list_clips_with_metadata(uid, video_id, highlights_map)
    
    return VideoInfoResponse(
        id=video_id,
        clips=clips,
        custom_prompt=highlights_data.get("custom_prompt"),
    )


@router.get("/user/videos", response_model=UserVideosResponse)
async def get_user_videos(
    user: Dict[str, Any] = Depends(get_current_user),
) -> UserVideosResponse:
    """List all videos for the authenticated user."""
    uid = user["uid"]
    videos = saas.list_user_videos(uid)
    return UserVideosResponse(videos=videos)


@router.get("/videos/{video_id}/clips/{clip_name}")
async def get_clip(
    video_id: str,
    clip_name: str,
    request: Request,
    user: Dict[str, Any] = Depends(get_current_user),
) -> Response:
    """Stream a video clip file with support for HTTP range requests."""
    from app.config import logger
    
    # Validate video_id and clip_name to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    # Validate clip_name - should only contain safe characters
    if not clip_name or ".." in clip_name or "/" in clip_name or "\\" in clip_name:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid clip name")
    
    uid = user["uid"]
    
    # Check ownership
    is_owner = saas.user_owns_video(uid, video_id)
    highlights_data = storage.load_highlights(uid, video_id)
    
    if not is_owner and not highlights_data:
        logger.warning(f"Video not found for user {uid}: {video_id}")
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    # Construct the R2 key
    key = f"{uid}/{video_id}/clips/{clip_name}"
    
    # Get the object from R2
    obj = storage.get_object(key)
    if not obj:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Clip not found")
    
    # Get file size
    content_length = obj.get("ContentLength", 0)
    
    # Determine content type
    content_type = obj.get("ContentType", "video/mp4")
    if not content_type.startswith("video/") and not content_type.startswith("image/"):
        # Default to video/mp4 for .mp4 files, image/jpeg for .jpg files
        if clip_name.lower().endswith(".mp4"):
            content_type = "video/mp4"
        elif clip_name.lower().endswith(".jpg") or clip_name.lower().endswith(".jpeg"):
            content_type = "image/jpeg"
    
    # Handle range requests for video seeking
    range_header = request.headers.get("range")
    if range_header:
        # Parse range header (e.g., "bytes=0-1023")
        try:
            range_match = range_header.replace("bytes=", "").split("-")
            start = int(range_match[0]) if range_match[0] else 0
            end = int(range_match[1]) if range_match[1] else content_length - 1
            
            if start < 0 or end >= content_length or start > end:
                raise HTTPException(
                    status_code=status.HTTP_416_REQUESTED_RANGE_NOT_SATISFIABLE,
                    detail="Invalid range",
                    headers={"Content-Range": f"bytes */{content_length}"}
                )
            
            # Get range from R2
            range_obj = storage.get_object(key, Range=f"bytes={start}-{end}")
            if not range_obj:
                raise HTTPException(status_code=status.HTTP_416_REQUESTED_RANGE_NOT_SATISFIABLE)
            
            def generate_range():
                body = range_obj["Body"]
                while True:
                    chunk = body.read(8192)
                    if not chunk:
                        break
                    yield chunk
            
            content_range = f"bytes {start}-{end}/{content_length}"
            return StreamingResponse(
                generate_range(),
                status_code=206,  # Partial Content
                media_type=content_type,
                headers={
                    "Content-Range": content_range,
                    "Accept-Ranges": "bytes",
                    "Content-Length": str(end - start + 1),
                    "Cache-Control": "public, max-age=3600",
                }
            )
        except (ValueError, IndexError):
            # Invalid range header, fall through to full file
            pass
    
    # Stream the full file
    def generate():
        body = obj["Body"]
        while True:
            chunk = body.read(8192)  # Read in 8KB chunks
            if not chunk:
                break
            yield chunk
    
    return StreamingResponse(
        generate(),
        media_type=content_type,
        headers={
            "Accept-Ranges": "bytes",
            "Content-Length": str(content_length),
            "Cache-Control": "public, max-age=3600",
        }
    )
