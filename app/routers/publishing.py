from typing import Dict, Any

from fastapi import APIRouter, Depends, HTTPException, status

from app.core.firebase_client import get_current_user
from app.core import saas, storage
from app.core.tiktok_client import publish_clip_to_tiktok as publish_clip_to_tiktok_service
from app.core.security import ValidationError, validate_video_id, validate_clip_name
from app.schemas import TikTokPublishRequest, TikTokPublishResponse

router = APIRouter(prefix="/api", tags=["Publishing"])

@router.post("/videos/{video_id}/clips/{clip_name}/publish/tiktok")
async def publish_clip_to_tiktok(
    video_id: str,
    clip_name: str,
    payload: TikTokPublishRequest | None = None,
    user: Dict[str, Any] = Depends(get_current_user),
) -> TikTokPublishResponse:
    """Publish a clip to TikTok."""
    # Validate path parameters
    try:
        video_id = validate_video_id(video_id)
        clip_name = validate_clip_name(clip_name)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    uid = user["uid"]
    
    if not saas.user_owns_video(uid, video_id):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    settings = saas.get_user_settings(uid)
    
    title = payload.title if payload else ""
    description = payload.description if payload else ""
    
    # Fallback to highlights metadata if title/description are not provided
    if not title or not description:
        highlights_data = storage.load_highlights(uid, video_id)
        for h in highlights_data.get("highlights", []):
            try:
                parts = clip_name.split("_")
                if len(parts) >= 3 and parts[0] == "clip":
                    clip_id = int(parts[2])
                    if int(h.get("id")) == clip_id:
                        if not title:
                            title = h.get("title", "")
                        if not description:
                            description = h.get("description", "")
                        break
            except Exception:
                continue
    
    s3_key = f"{uid}/{video_id}/clips/{clip_name}"
    result = await publish_clip_to_tiktok_service(settings, s3_key, title or "", description or "")
    return TikTokPublishResponse(**result) if isinstance(result, dict) else TikTokPublishResponse(success=True)
