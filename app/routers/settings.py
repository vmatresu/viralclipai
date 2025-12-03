from typing import Dict, Any

from fastapi import APIRouter, Depends

from app.core.firebase_client import get_current_user
from app.core import saas
from app.schemas import UserSettingsResponse, SettingsUpdateRequest, SettingsUpdateResponse

router = APIRouter(prefix="/api/settings", tags=["Settings"])

@router.get("", response_model=UserSettingsResponse)
async def get_settings(
    user: Dict[str, Any] = Depends(get_current_user),
) -> UserSettingsResponse:
    """Get user settings and plan information."""
    uid = user["uid"]
    settings = saas.get_user_settings(uid)
    plan_id, max_clips = saas.get_plan_limits_for_user(uid)
    used = saas.get_monthly_usage(uid)
    # Get user role for frontend to determine admin access
    user_role = None
    if saas.is_super_admin(uid):
        user_role = "superadmin"
    return UserSettingsResponse(
        settings=settings,
        plan=plan_id,
        max_clips_per_month=max_clips,
        clips_used_this_month=used,
        role=user_role,
    )


@router.post("", response_model=SettingsUpdateResponse)
async def update_settings(
    payload: SettingsUpdateRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> SettingsUpdateResponse:
    """Update user settings."""
    uid = user["uid"]
    updated = saas.update_user_settings(uid, payload.settings)
    return SettingsUpdateResponse(settings=updated)
