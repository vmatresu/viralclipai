from datetime import datetime, timezone

from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from pydantic import ValidationError as PydanticValidationError

from app.config import logger
from app.core.firebase_client import verify_id_token
from app.core import saas
from app.core.workflow import process_video_workflow
from app.core.security import (
    check_ws_rate_limit,
    log_security_event,
)
from app.schemas import WSProcessRequest, WSReprocessRequest
from app.core.reprocessing import reprocess_scenes_workflow

router = APIRouter()

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
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": f"Invalid request: {error_msg}",
                    "timestamp": timestamp,
                }
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
        except Exception as e:
            logger.warning(f"WebSocket auth failed for token starting with {request_data.token[:10]}... Error: {str(e)}")
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": f"Authentication failed: {str(e)}",
                    "timestamp": timestamp,
                }
            )
            log_security_event("ws_auth_failed", request=websocket)
            return
        
        uid = decoded.get("uid")
        email = decoded.get("email")
        
        if not uid:
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": "Invalid authentication payload",
                    "timestamp": timestamp,
                }
            )
            return
        
        # Create/update user record
        saas.get_or_create_user(uid, email)
        
        logger.info("WebSocket processing started for user %s", uid)
        
        # Process video with validated inputs
        await process_video_workflow(
            websocket,
            request_data.url,
            request_data.styles,
            user_id=uid,
            custom_prompt=request_data.prompt,
        )
        
    except WebSocketDisconnect:
        logger.info("WebSocket disconnected for user %s", uid or "unknown")
    except Exception as e:
        logger.exception("WebSocket error for user %s: %s", uid or "unknown", e)
        try:
            # Don't expose internal errors to client
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json({
                "type": "error",
                "message": "An error occurred while processing your video. Please try again.",
                "timestamp": timestamp,
            })
        except Exception:
            pass


@router.websocket("/ws/reprocess")
async def websocket_reprocess_endpoint(websocket: WebSocket):
    """Reprocess scenes via WebSocket with real-time progress updates."""
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
            request_data = WSReprocessRequest.model_validate(raw_data)
        except PydanticValidationError as e:
            errors = e.errors()
            error_msg = "; ".join(f"{err['loc'][0]}: {err['msg']}" for err in errors[:3])
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": f"Invalid request: {error_msg}",
                    "timestamp": timestamp,
                }
            )
            log_security_event(
                "ws_reprocess_validation_failed",
                request=websocket,
                details={"errors": str(errors)[:500]}
            )
            return
        
        # Authenticate
        try:
            decoded = verify_id_token(request_data.token)
        except Exception as e:
            logger.warning(f"WebSocket reprocess auth failed: {str(e)}")
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": f"Authentication failed: {str(e)}",
                    "timestamp": timestamp,
                }
            )
            log_security_event("ws_reprocess_auth_failed", request=websocket)
            return
        
        uid = decoded.get("uid")
        email = decoded.get("email")
        
        if not uid:
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": "Invalid authentication payload",
                    "timestamp": timestamp,
                }
            )
            return
        
        # Create/update user record
        saas.get_or_create_user(uid, email)
        
        # Check ownership
        if not saas.user_owns_video(uid, request_data.video_id):
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": "Video not found or access denied",
                    "timestamp": timestamp,
                }
            )
            return
        
        # Check if video is currently processing
        if saas.is_video_processing(uid, request_data.video_id):
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": "Video is currently processing. Please wait for it to complete before reprocessing.",
                    "timestamp": timestamp,
                }
            )
            return
        
        # Check plan restrictions (pro/enterprise only)
        if not saas.has_pro_or_enterprise_plan(uid):
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json(
                {
                    "type": "error",
                    "message": "Scene reprocessing is only available for Pro and Enterprise plans. Please upgrade to access this feature.",
                    "timestamp": timestamp,
                }
            )
            return
        
        # Update video status to processing to prevent concurrent submissions
        saas.update_video_status(uid, request_data.video_id, "processing")
        
        logger.info("WebSocket reprocessing started for user %s, video %s", uid, request_data.video_id)
        
        # Reprocess scenes with validated inputs
        await reprocess_scenes_workflow(
            websocket,
            request_data.video_id,
            request_data.scene_ids,
            request_data.styles,
            user_id=uid,
            crop_mode=request_data.crop_mode,
            target_aspect=request_data.target_aspect,
        )
        
    except WebSocketDisconnect:
        logger.info("WebSocket reprocess disconnected for user %s", uid or "unknown")
    except Exception as e:
        logger.exception("WebSocket reprocess error for user %s: %s", uid or "unknown", e)
        try:
            # Don't expose internal errors to client
            timestamp = datetime.now(timezone.utc).isoformat()
            await websocket.send_json({
                "type": "error",
                "message": "An error occurred while reprocessing scenes. Please try again.",
                "timestamp": timestamp,
            })
        except Exception:
            pass
