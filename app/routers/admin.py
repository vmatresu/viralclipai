from typing import Dict, Any

from fastapi import APIRouter, Depends, HTTPException, status

from app.config import logger
from app.core.firebase_client import get_current_user
from app.core import saas
from app.core.security import log_security_event
from app.schemas import AdminPromptRequest, AdminPromptResponse

router = APIRouter(prefix="/api/admin", tags=["Admin"])

@router.get("/prompt", response_model=AdminPromptResponse)
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


@router.post("/prompt", response_model=AdminPromptResponse)
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
