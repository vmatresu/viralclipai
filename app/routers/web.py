import json
from fastapi import (
    APIRouter,
    Body,
    Depends,
    HTTPException,
    WebSocket,
    WebSocketDisconnect,
    status,
)

from app.core.firebase_client import get_current_user, verify_id_token
from app.core import saas, storage
from app.core.tiktok_client import publish_clip_to_tiktok as publish_clip_to_tiktok_service
from app.core.workflow import process_video_workflow

router = APIRouter()


@router.websocket("/ws/process")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    try:
        data = await websocket.receive_json()
        token = data.get("token")
        url = data.get("url")
        style = data.get("style", "split")

        if not token:
            await websocket.send_json(
                {"type": "error", "message": "Authentication required"}
            )
            return

        try:
            decoded = verify_id_token(token)
        except Exception:
            await websocket.send_json(
                {"type": "error", "message": "Invalid or expired authentication"}
            )
            return

        uid = decoded.get("uid")
        email = decoded.get("email")
        if not uid:
            await websocket.send_json(
                {"type": "error", "message": "Invalid authentication payload"}
            )
            return

        saas.get_or_create_user(uid, email)

        if not url:
            await websocket.send_json({"type": "error", "message": "No URL provided"})
            return

        await process_video_workflow(websocket, url, style, user_id=uid)
    except WebSocketDisconnect:
        pass
    except Exception as e:
        try:
            await websocket.send_json({"type": "error", "message": str(e)})
        except Exception:
            pass


@router.get("/api/videos/{video_id}")
async def get_video_info(video_id: str, user=Depends(get_current_user)):
    uid = user["uid"]

    if not saas.user_owns_video(uid, video_id):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")

    # Load highlights metadata from S3
    highlights_data = storage.load_highlights(uid, video_id)
    highlights_map = {}
    for h in highlights_data.get("highlights", []):
        h_id = h.get("id")
        if h_id is not None:
            highlights_map[int(h_id)] = {
                "title": h.get("title", ""),
                "description": h.get("description", ""),
            }

    clips = storage.list_clips_with_metadata(uid, video_id, highlights_map)
    return {"id": video_id, "clips": clips}


@router.get("/api/user/videos")
async def get_user_videos(user=Depends(get_current_user)):
    uid = user["uid"]
    videos = saas.list_user_videos(uid)
    return {"videos": videos}


@router.get("/api/settings")
async def get_settings(user=Depends(get_current_user)):
    uid = user["uid"]
    settings = saas.get_user_settings(uid)
    plan_id, max_clips = saas.get_plan_limits_for_user(uid)
    used = saas.get_monthly_usage(uid)
    return {
        "settings": settings,
        "plan": plan_id,
        "max_clips_per_month": max_clips,
        "clips_used_this_month": used,
    }


@router.post("/api/settings")
async def update_settings(
    payload: dict = Body(...),
    user=Depends(get_current_user),
):
    uid = user["uid"]
    settings = payload.get("settings") or {}
    updated = saas.update_user_settings(uid, settings)
    return {"settings": updated}


@router.post("/api/videos/{video_id}/clips/{clip_name}/publish/tiktok")
async def publish_clip_to_tiktok(
    video_id: str,
    clip_name: str,
    payload: dict = Body(None),
    user=Depends(get_current_user),
):
    uid = user["uid"]

    if not saas.user_owns_video(uid, video_id):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")

    settings = saas.get_user_settings(uid)

    title = ""
    description = ""
    if payload:
        title = payload.get("title") or ""
        description = payload.get("description") or ""

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
    result = await publish_clip_to_tiktok_service(settings, s3_key, title, description)
    return result
