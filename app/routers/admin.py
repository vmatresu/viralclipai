from typing import Dict, Any, List

from fastapi import APIRouter, Depends, HTTPException, status

from app.config import logger
from app.core.firebase_client import get_current_user
from app.core import saas
from app.core.plans.models import Plan, PlanStatus
from app.core.plans.service import PlanService
from app.core.security import log_security_event
from app.schemas import (
    AdminPromptRequest,
    AdminPromptResponse,
    PlanCreateRequest,
    PlanUpdateRequest,
    PlanResponse,
    PlansListResponse,
)

router = APIRouter(prefix="/api/admin", tags=["Admin"])


def require_superadmin(user: Dict[str, Any]) -> str:
    """Dependency to require superadmin role."""
    uid = user["uid"]
    if not saas.is_super_admin(uid):
        log_security_event(
            "unauthorized_admin_access",
            user_id=uid,
            details={"endpoint": "admin_endpoint"}
        )
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Not authorized")
    return uid

@router.get("/prompt", response_model=AdminPromptResponse)
async def get_admin_prompt(
    user: Dict[str, Any] = Depends(get_current_user),
) -> AdminPromptResponse:
    """Get the global admin prompt (superadmin only)."""
    uid = require_superadmin(user)
    prompt = saas.get_global_prompt() or ""
    return AdminPromptResponse(prompt=prompt)


@router.post("/prompt", response_model=AdminPromptResponse)
async def update_admin_prompt(
    payload: AdminPromptRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> AdminPromptResponse:
    """Update the global admin prompt (superadmin only)."""
    uid = require_superadmin(user)
    updated = saas.set_global_prompt(uid, payload.prompt)
    logger.info("Admin prompt updated by user %s", uid)
    return AdminPromptResponse(prompt=updated)


# -----------------------------------------------------------------------------
# Plan Management Endpoints
# -----------------------------------------------------------------------------

@router.get("/plans", response_model=PlansListResponse)
async def list_plans(
    include_inactive: bool = False,
    user: Dict[str, Any] = Depends(get_current_user),
) -> PlansListResponse:
    """List all plans (superadmin only)."""
    uid = require_superadmin(user)
    plan_service = PlanService()
    plans = plan_service.get_all_plans(include_inactive=include_inactive)
    
    plan_responses = [
        PlanResponse(
            id=plan.id,
            name=plan.name,
            description=plan.description,
            price_monthly=plan.price_monthly,
            price_yearly=plan.price_yearly,
            status=plan.status.value,
            limits=plan.limits,
            features=plan.features,
            created_at=plan.created_at,
            updated_at=plan.updated_at,
            created_by=plan.created_by,
            updated_by=plan.updated_by,
        )
        for plan in plans
    ]
    
    return PlansListResponse(plans=plan_responses)


@router.get("/plans/{plan_id}", response_model=PlanResponse)
async def get_plan(
    plan_id: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> PlanResponse:
    """Get a specific plan by ID (superadmin only)."""
    uid = require_superadmin(user)
    plan_service = PlanService()
    plan = plan_service.get_plan(plan_id.lower())
    
    if not plan:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Plan '{plan_id}' not found"
        )
    
    return PlanResponse(
        id=plan.id,
        name=plan.name,
        description=plan.description,
        price_monthly=plan.price_monthly,
        price_yearly=plan.price_yearly,
        status=plan.status.value,
        limits=plan.limits,
        features=plan.features,
        created_at=plan.created_at,
        updated_at=plan.updated_at,
        created_by=plan.created_by,
        updated_by=plan.updated_by,
    )


@router.post("/plans", response_model=PlanResponse, status_code=status.HTTP_201_CREATED)
async def create_plan(
    payload: PlanCreateRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> PlanResponse:
    """Create a new plan (superadmin only)."""
    uid = require_superadmin(user)
    plan_service = PlanService()
    
    try:
        plan = Plan(
            id=payload.id.lower(),
            name=payload.name,
            description=payload.description,
            price_monthly=payload.price_monthly,
            price_yearly=payload.price_yearly,
            status=PlanStatus(payload.status.lower()),
            limits=payload.limits,
            features=payload.features,
        )
        
        created_plan = plan_service.create_plan(plan, created_by=uid)
        logger.info("Plan '%s' created by user %s", created_plan.id, uid)
        
        return PlanResponse(
            id=created_plan.id,
            name=created_plan.name,
            description=created_plan.description,
            price_monthly=created_plan.price_monthly,
            price_yearly=created_plan.price_yearly,
            status=created_plan.status.value,
            limits=created_plan.limits,
            features=created_plan.features,
            created_at=created_plan.created_at,
            updated_at=created_plan.updated_at,
            created_by=created_plan.created_by,
            updated_by=created_plan.updated_by,
        )
    except ValueError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(e)
        )


@router.put("/plans/{plan_id}", response_model=PlanResponse)
async def update_plan(
    plan_id: str,
    payload: PlanUpdateRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> PlanResponse:
    """Update an existing plan (superadmin only)."""
    uid = require_superadmin(user)
    plan_service = PlanService()
    
    # Build update dictionary from non-None fields
    updates: Dict[str, Any] = {}
    if payload.name is not None:
        updates["name"] = payload.name
    if payload.description is not None:
        updates["description"] = payload.description
    if payload.price_monthly is not None:
        updates["price_monthly"] = payload.price_monthly
    if payload.price_yearly is not None:
        updates["price_yearly"] = payload.price_yearly
    if payload.limits is not None:
        updates["limits"] = payload.limits
    if payload.features is not None:
        updates["features"] = payload.features
    if payload.status is not None:
        updates["status"] = PlanStatus(payload.status.lower()).value
    
    if not updates:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="No fields to update"
        )
    
    try:
        updated_plan = plan_service.update_plan(plan_id.lower(), updates, updated_by=uid)
        
        if not updated_plan:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail=f"Plan '{plan_id}' not found"
            )
        
        logger.info("Plan '%s' updated by user %s", plan_id, uid)
        
        return PlanResponse(
            id=updated_plan.id,
            name=updated_plan.name,
            description=updated_plan.description,
            price_monthly=updated_plan.price_monthly,
            price_yearly=updated_plan.price_yearly,
            status=updated_plan.status.value,
            limits=updated_plan.limits,
            features=updated_plan.features,
            created_at=updated_plan.created_at,
            updated_at=updated_plan.updated_at,
            created_by=updated_plan.created_by,
            updated_by=updated_plan.updated_by,
        )
    except ValueError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(e)
        )


@router.delete("/plans/{plan_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_plan(
    plan_id: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> None:
    """Delete (archive) a plan (superadmin only)."""
    uid = require_superadmin(user)
    plan_service = PlanService()
    
    try:
        deleted = plan_service.delete_plan(plan_id.lower())
        if not deleted:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail=f"Plan '{plan_id}' not found"
            )
        logger.info("Plan '%s' archived by user %s", plan_id, uid)
    except ValueError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(e)
        )
