from typing import Dict, Any

from fastapi import (
    APIRouter,
    Depends,
    HTTPException,
    WebSocket,
    WebSocketDisconnect,
    status,
)
from pydantic import ValidationError as PydanticValidationError

from app.config import logger
from app.core.firebase_client import get_current_user, verify_id_token
from app.core import saas, storage
from app.core.tiktok_client import publish_clip_to_tiktok as publish_clip_to_tiktok_service
from app.core.workflow import process_video_workflow
from app.core.security import (
    ValidationError,
    check_ws_rate_limit,
    log_security_event,
    validate_clip_name,
    validate_video_id,
)
from app.schemas import (
    AdminPromptRequest,
    AdminPromptResponse,
    SettingsUpdateRequest,
    SettingsUpdateResponse,
    TikTokPublishRequest,
    TikTokPublishResponse,
    UserSettingsResponse,
    UserVideosResponse,
    VideoInfoResponse,
    WSProcessRequest,
)

router = APIRouter()


# -----------------------------------------------------------------------------
# WebSocket Endpoint (with rate limiting and validation)
# -----------------------------------------------------------------------------

@router.websocket("/ws/process")
async def websocket_endpoint(websocket: WebSocket):
    """Process video via WebSocket with real-time progress updates."""
    # Check rate limit before accepting connection
    if not await check_ws_rate_limit(websocket, user_id=None):
        await websocket.close(code=1008, reason="Rate limit exceeded")
        return
    
    await websocket.accept()
    uid: str | None = None
    
    try:
        # Receive and validate request data
        raw_data = await websocket.receive_json()
        
        # Validate with Pydantic schema
        try:
            request_data = WSProcessRequest.model_validate(raw_data)
        except PydanticValidationError as e:
            errors = e.errors()
            error_msg = "; ".join(f"{err['loc'][0]}: {err['msg']}" for err in errors[:3])
            await websocket.send_json(
                {"type": "error", "message": f"Invalid request: {error_msg}"}
            )
            log_security_event(
                "ws_validation_failed",
                request=websocket,
                details={"errors": str(errors)[:500]}
            )
            return
        
        # Authenticate
        try:
            decoded = verify_id_token(request_data.token)
        except Exception:
            await websocket.send_json(
                {"type": "error", "message": "Invalid or expired authentication"}
            )
            log_security_event("ws_auth_failed", request=websocket)
            return
        
        uid = decoded.get("uid")
        email = decoded.get("email")
        
        if not uid:
            await websocket.send_json(
                {"type": "error", "message": "Invalid authentication payload"}
            )
            return
        
        # Create/update user record
        saas.get_or_create_user(uid, email)
        
        logger.info("WebSocket processing started for user %s", uid)
        
        # Process video with validated inputs
        await process_video_workflow(
            websocket,
            request_data.url,
            request_data.style,
            user_id=uid,
            custom_prompt=request_data.prompt,
        )
        
    except WebSocketDisconnect:
        logger.info("WebSocket disconnected for user %s", uid or "unknown")
    except Exception as e:
        logger.exception("WebSocket error for user %s: %s", uid or "unknown", e)
        try:
            # Don't expose internal errors to client
            await websocket.send_json({
                "type": "error",
                "message": "An error occurred while processing your video. Please try again."
            })
        except Exception:
            pass


# -----------------------------------------------------------------------------
# Video Endpoints
# -----------------------------------------------------------------------------

@router.get("/api/videos/{video_id}", response_model=VideoInfoResponse)
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


@router.get("/api/user/videos", response_model=UserVideosResponse)
async def get_user_videos(
    user: Dict[str, Any] = Depends(get_current_user),
) -> UserVideosResponse:
    """List all videos for the authenticated user."""
    uid = user["uid"]
    videos = saas.list_user_videos(uid)
    return UserVideosResponse(videos=videos)


# -----------------------------------------------------------------------------
# Settings Endpoints
# -----------------------------------------------------------------------------

@router.get("/api/settings", response_model=UserSettingsResponse)
async def get_settings(
    user: Dict[str, Any] = Depends(get_current_user),
) -> UserSettingsResponse:
    """Get user settings and plan information."""
    uid = user["uid"]
    settings = saas.get_user_settings(uid)
    plan_id, max_clips = saas.get_plan_limits_for_user(uid)
    used = saas.get_monthly_usage(uid)
    return UserSettingsResponse(
        settings=settings,
        plan=plan_id,
        max_clips_per_month=max_clips,
        clips_used_this_month=used,
    )


@router.post("/api/settings", response_model=SettingsUpdateResponse)
async def update_settings(
    payload: SettingsUpdateRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> SettingsUpdateResponse:
    """Update user settings."""
    uid = user["uid"]
    updated = saas.update_user_settings(uid, payload.settings)
    return SettingsUpdateResponse(settings=updated)


# -----------------------------------------------------------------------------
# TikTok Publishing Endpoint
# -----------------------------------------------------------------------------

@router.post("/api/videos/{video_id}/clips/{clip_name}/publish/tiktok")
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


# -----------------------------------------------------------------------------
# Admin Endpoints
# -----------------------------------------------------------------------------

@router.get("/api/admin/prompt", response_model=AdminPromptResponse)
async def get_admin_prompt(
    user: Dict[str, Any] = Depends(get_current_user),
) -> AdminPromptResponse:
    """Get the global admin prompt (superadmin only)."""
    uid = user["uid"]
    if not saas.is_super_admin(uid):
        log_security_event(
            "unauthorized_admin_access",
            user_id=uid,
            details={"endpoint": "get_admin_prompt"}
        )
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Not authorized")
    
    prompt = saas.get_global_prompt() or ""
    return AdminPromptResponse(prompt=prompt)


@router.post("/api/admin/prompt", response_model=AdminPromptResponse)
async def update_admin_prompt(
    payload: AdminPromptRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> AdminPromptResponse:
    """Update the global admin prompt (superadmin only)."""
    uid = user["uid"]
    if not saas.is_super_admin(uid):
        log_security_event(
            "unauthorized_admin_access",
            user_id=uid,
            details={"endpoint": "update_admin_prompt"}
        )
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Not authorized")
    
    updated = saas.set_global_prompt(uid, payload.prompt)
    logger.info("Admin prompt updated by user %s", uid)
    return AdminPromptResponse(prompt=updated)
