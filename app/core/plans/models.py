"""
Plan Models

Type-safe models for plan definitions and limits.
"""

from datetime import datetime
from typing import Dict, Optional
from enum import Enum

from pydantic import BaseModel, Field, field_validator


class PlanStatus(str, Enum):
    """Plan status enumeration."""
    ACTIVE = "active"
    INACTIVE = "inactive"
    ARCHIVED = "archived"


class PlanLimit(BaseModel):
    """Represents a single limit for a plan."""
    
    name: str = Field(..., description="Limit name (e.g., 'max_clips_per_month')")
    value: int = Field(..., ge=0, description="Limit value")
    description: Optional[str] = Field(None, description="Human-readable description")
    
    @field_validator("name")
    @classmethod
    def validate_name(cls, v: str) -> str:
        """Validate limit name format."""
        if not v or not v.strip():
            raise ValueError("Limit name cannot be empty")
        # Allow alphanumeric, underscore, hyphen
        import re
        if not re.match(r"^[a-z][a-z0-9_-]*$", v.lower()):
            raise ValueError("Limit name must be lowercase alphanumeric with underscores/hyphens")
        return v.lower()


class Plan(BaseModel):
    """Represents a subscription plan with limits and metadata."""
    
    id: str = Field(..., description="Plan identifier (e.g., 'free', 'pro', 'studio')")
    name: str = Field(..., min_length=1, max_length=100, description="Display name")
    description: Optional[str] = Field(None, max_length=500, description="Plan description")
    price_monthly: Optional[float] = Field(None, ge=0, description="Monthly price in USD")
    price_yearly: Optional[float] = Field(None, ge=0, description="Yearly price in USD")
    status: PlanStatus = Field(default=PlanStatus.ACTIVE, description="Plan status")
    limits: Dict[str, int] = Field(
        default_factory=dict,
        description="Plan limits as key-value pairs (e.g., {'max_clips_per_month': 20})"
    )
    features: list[str] = Field(
        default_factory=list,
        description="List of feature names included in this plan"
    )
    created_at: datetime = Field(default_factory=datetime.utcnow, description="Creation timestamp")
    updated_at: datetime = Field(default_factory=datetime.utcnow, description="Last update timestamp")
    created_by: Optional[str] = Field(None, description="User ID who created the plan")
    updated_by: Optional[str] = Field(None, description="User ID who last updated the plan")
    
    @field_validator("id")
    @classmethod
    def validate_id(cls, v: str) -> str:
        """Validate plan ID format."""
        if not v or not v.strip():
            raise ValueError("Plan ID cannot be empty")
        # Allow lowercase alphanumeric, underscore, hyphen
        import re
        if not re.match(r"^[a-z][a-z0-9_-]*$", v.lower()):
            raise ValueError("Plan ID must be lowercase alphanumeric with underscores/hyphens")
        return v.lower()
    
    @field_validator("limits")
    @classmethod
    def validate_limits(cls, v: Dict[str, int]) -> Dict[str, int]:
        """Validate limits dictionary."""
        validated = {}
        for key, value in v.items():
            if not isinstance(value, int) or value < 0:
                raise ValueError(f"Limit '{key}' must be a non-negative integer")
            validated[key.lower()] = value
        return validated
    
    def get_limit(self, limit_name: str, default: int = 0) -> int:
        """Get a specific limit value with optional default."""
        return self.limits.get(limit_name.lower(), default)
    
    def has_feature(self, feature_name: str) -> bool:
        """Check if plan includes a specific feature."""
        return feature_name.lower() in [f.lower() for f in self.features]
    
    def to_firestore_dict(self) -> Dict:
        """Convert to Firestore-compatible dictionary."""
        return {
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "price_monthly": self.price_monthly,
            "price_yearly": self.price_yearly,
            "status": self.status.value,
            "limits": self.limits,
            "features": self.features,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "created_by": self.created_by,
            "updated_by": self.updated_by,
        }
    
    @classmethod
    def from_firestore_dict(cls, data: Dict) -> "Plan":
        """Create Plan from Firestore document."""
        from datetime import timezone
        
        # Handle datetime conversion from Firestore Timestamp
        created_at = data.get("created_at")
        if created_at and not isinstance(created_at, datetime):
            # Firestore Timestamp has timestamp() method
            if hasattr(created_at, "timestamp"):
                created_at = datetime.fromtimestamp(created_at.timestamp(), tz=timezone.utc)
            elif hasattr(created_at, "seconds"):
                # Firestore Timestamp has seconds and nanoseconds
                created_at = datetime.fromtimestamp(created_at.seconds, tz=timezone.utc)
            else:
                created_at = datetime.now(timezone.utc)
        elif not created_at:
            created_at = datetime.now(timezone.utc)
        
        updated_at = data.get("updated_at")
        if updated_at and not isinstance(updated_at, datetime):
            # Firestore Timestamp has timestamp() method
            if hasattr(updated_at, "timestamp"):
                updated_at = datetime.fromtimestamp(updated_at.timestamp(), tz=timezone.utc)
            elif hasattr(updated_at, "seconds"):
                # Firestore Timestamp has seconds and nanoseconds
                updated_at = datetime.fromtimestamp(updated_at.seconds, tz=timezone.utc)
            else:
                updated_at = datetime.now(timezone.utc)
        elif not updated_at:
            updated_at = datetime.now(timezone.utc)
        
        return cls(
            id=data.get("id", ""),
            name=data.get("name", ""),
            description=data.get("description"),
            price_monthly=data.get("price_monthly"),
            price_yearly=data.get("price_yearly"),
            status=PlanStatus(data.get("status", PlanStatus.ACTIVE.value)),
            limits=data.get("limits", {}),
            features=data.get("features", []),
            created_at=created_at,
            updated_at=updated_at,
            created_by=data.get("created_by"),
            updated_by=data.get("updated_by"),
        )

