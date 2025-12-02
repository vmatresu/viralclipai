from typing import Dict, Any

from fastapi import APIRouter, Depends, HTTPException, status

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
    
    if not saas.user_owns_video(uid, video_id):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    # Load highlights metadata
    highlights_data = storage.load_highlights(uid, video_id)
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
