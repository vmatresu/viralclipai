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
from app.schemas import WSProcessRequest

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
        except Exception as e:
            logger.warning(f"WebSocket auth failed for token starting with {request_data.token[:10]}... Error: {str(e)}")
            await websocket.send_json(
                {"type": "error", "message": f"Authentication failed: {str(e)}"}
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
