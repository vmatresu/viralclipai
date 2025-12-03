"""
Plan Service

Business logic layer for plan management.
Provides high-level operations following SOLID principles.
"""

import logging
from typing import Dict, List, Optional, Tuple

from app.core.plans.models import Plan, PlanStatus
from app.core.plans.repository import PlanRepository
from app.config import logger


class PlanService:
    """
    Service layer for plan management operations.
    
    Provides business logic and validation separate from data access.
    """
    
    def __init__(self, repository: Optional[PlanRepository] = None):
        """Initialize plan service with optional repository."""
        self._repo = repository or PlanRepository
    
    def get_plan(self, plan_id: str) -> Optional[Plan]:
        """
        Get a plan by ID.
        
        Args:
            plan_id: Plan identifier
            
        Returns:
            Plan if found, None otherwise
        """
        return self._repo.get_by_id(plan_id)
    
    def get_all_plans(self, include_inactive: bool = False) -> List[Plan]:
        """
        Get all plans.
        
        Args:
            include_inactive: Whether to include inactive plans
            
        Returns:
            List of plans
        """
        return self._repo.get_all(include_inactive=include_inactive)
    
    def get_active_plans(self) -> List[Plan]:
        """Get only active plans."""
        return self._repo.get_all(include_inactive=False)
    
    def get_plan_limits(self, plan_id: str) -> Tuple[str, Dict[str, int]]:
        """
        Get plan limits for a given plan ID.
        
        Args:
            plan_id: Plan identifier
            
        Returns:
            Tuple of (plan_id, limits_dict)
            
        Raises:
            ValueError: If plan not found
        """
        plan = self.get_plan(plan_id)
        if not plan:
            # Fallback to default plan
            default_id = self._repo.get_default_plan_id()
            plan = self.get_plan(default_id)
            if not plan:
                raise ValueError(f"Plan '{plan_id}' not found and default plan unavailable")
            plan_id = default_id
        
        return plan_id, plan.limits
    
    def get_plan_limit_value(self, plan_id: str, limit_name: str, default: int = 0) -> int:
        """
        Get a specific limit value for a plan.
        
        Args:
            plan_id: Plan identifier
            limit_name: Name of the limit (e.g., 'max_clips_per_month')
            default: Default value if limit not found
            
        Returns:
            Limit value
        """
        _, limits = self.get_plan_limits(plan_id)
        return limits.get(limit_name.lower(), default)
    
    def create_plan(self, plan: Plan, created_by: str) -> Plan:
        """
        Create a new plan.
        
        Args:
            plan: Plan to create
            created_by: User ID creating the plan
            
        Returns:
            Created plan
            
        Raises:
            ValueError: If plan validation fails or plan exists
        """
        # Validate plan
        if not plan.id or not plan.name:
            raise ValueError("Plan ID and name are required")
        
        if not plan.limits:
            logger.warning("Creating plan '%s' with no limits", plan.id)
        
        return self._repo.create(plan, created_by)
    
    def update_plan(
        self,
        plan_id: str,
        updates: Dict,
        updated_by: str
    ) -> Optional[Plan]:
        """
        Update an existing plan.
        
        Args:
            plan_id: Plan identifier
            updates: Dictionary of fields to update
            updated_by: User ID updating the plan
            
        Returns:
            Updated plan if found, None otherwise
            
        Raises:
            ValueError: If updates are invalid
        """
        # Validate updates
        if "id" in updates:
            raise ValueError("Cannot update plan ID")
        
        if "status" in updates:
            try:
                PlanStatus(updates["status"])
            except ValueError:
                raise ValueError(f"Invalid status: {updates['status']}")
        
        if "limits" in updates:
            limits = updates["limits"]
            if not isinstance(limits, dict):
                raise ValueError("Limits must be a dictionary")
            for key, value in limits.items():
                if not isinstance(value, int) or value < 0:
                    raise ValueError(f"Limit '{key}' must be a non-negative integer")
        
        return self._repo.update(plan_id, updates, updated_by)
    
    def delete_plan(self, plan_id: str) -> bool:
        """
        Delete (archive) a plan.
        
        Args:
            plan_id: Plan identifier
            
        Returns:
            True if deleted, False if not found
        """
        # Prevent deleting default plan
        if plan_id.lower() == self._repo.get_default_plan_id():
            raise ValueError("Cannot delete the default plan")
        
        return self._repo.delete(plan_id)
    
    def ensure_default_plans_exist(self) -> None:
        """
        Ensure default plans exist in Firestore.
        Called during initialization or migration.
        """
        default_plans = [
            Plan(
                id="free",
                name="Free",
                description="Free tier for testing and personal use",
                price_monthly=0.0,
                status=PlanStatus.ACTIVE,
                limits={"max_clips_per_month": 20},
                features=["ai_highlight_detection", "basic_support"],
            ),
            Plan(
                id="pro",
                name="Pro",
                description="Professional plan for content creators",
                price_monthly=29.0,
                status=PlanStatus.ACTIVE,
                limits={"max_clips_per_month": 500},
                features=[
                    "ai_highlight_detection",
                    "priority_processing",
                    "tiktok_publish",
                    "email_support",
                ],
            ),
            Plan(
                id="studio",
                name="Studio",
                description="Enterprise plan with custom limits and features",
                price_monthly=None,  # Custom pricing
                status=PlanStatus.ACTIVE,
                limits={"max_clips_per_month": 10000},  # High limit
                features=[
                    "ai_highlight_detection",
                    "priority_processing",
                    "tiktok_publish",
                    "team_accounts",
                    "custom_integrations",
                    "sla",
                ],
            ),
        ]
        
        for plan in default_plans:
            existing = self.get_plan(plan.id)
            if not existing:
                try:
                    self.create_plan(plan, created_by="system")
                    logger.info("Created default plan: %s", plan.id)
                except ValueError as e:
                    # Plan might have been created concurrently
                    logger.debug("Plan %s already exists: %s", plan.id, e)
                except Exception as e:
                    logger.error("Failed to create default plan %s: %s", plan.id, e)

