"""
Pydantic models for repository data structures.

Provides type safety and validation for repository operations.
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field, field_validator, model_validator


class ClipMetadata(BaseModel):
    """Clip metadata model with validation."""
    
    clip_id: str = Field(..., min_length=1, max_length=200)
    video_id: str = Field(..., min_length=1, max_length=100)
    user_id: str = Field(..., min_length=1, max_length=100)
    
    # Scene information
    scene_id: int = Field(..., ge=1, le=10000)
    scene_title: str = Field(..., min_length=1, max_length=500)
    scene_description: Optional[str] = Field(None, max_length=2000)
    
    # Clip metadata
    filename: str = Field(..., min_length=1, max_length=300)
    style: str = Field(..., min_length=1, max_length=50)
    priority: int = Field(default=99, ge=0, le=999)
    
    # Timing
    start_time: str = Field(..., min_length=8, max_length=15)  # HH:MM:SS or HH:MM:SS.mmm
    end_time: str = Field(..., min_length=8, max_length=15)
    duration_seconds: float = Field(..., ge=0, le=3600)
    
    # File information
    file_size_bytes: int = Field(default=0, ge=0)
    file_size_mb: float = Field(default=0.0, ge=0)
    has_thumbnail: bool = Field(default=False)
    
    # Storage references
    r2_key: str = Field(..., min_length=1, max_length=500)
    thumbnail_r2_key: Optional[str] = Field(None, max_length=500)
    
    # Status
    status: str = Field(default="processing")
    
    # Timestamps
    created_at: datetime
    completed_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None
    
    # Metadata
    created_by: str = Field(..., min_length=1, max_length=100)
    
    @model_validator(mode="before")
    @classmethod
    def calculate_file_size_mb(cls, data: Any) -> Any:
        """Calculate file_size_mb from file_size_bytes if not provided."""
        if isinstance(data, dict):
            if "file_size_mb" not in data or data.get("file_size_mb") == 0:
                if "file_size_bytes" in data and data["file_size_bytes"]:
                    data["file_size_mb"] = round(data["file_size_bytes"] / (1024 * 1024), 2)
        return data
    
    @field_validator("status")
    @classmethod
    def validate_status(cls, v: str) -> str:
        """Validate status value."""
        if v not in ["processing", "completed", "failed"]:
            raise ValueError(f"Invalid status: {v}. Must be one of: processing, completed, failed")
        return v
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ClipMetadata":
        """Create from Firestore document dictionary."""
        return cls.model_validate(data)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to Firestore-compatible dictionary."""
        data = self.model_dump(exclude_none=False)
        # Convert datetime to Firestore Timestamp-compatible format
        for key in ["created_at", "completed_at", "updated_at"]:
            if key in data and data[key] is not None:
                if isinstance(data[key], datetime):
                    data[key] = data[key]
        return data


class VideoMetadata(BaseModel):
    """Video metadata model with validation."""
    
    video_id: str = Field(..., min_length=1, max_length=100)
    user_id: str = Field(..., min_length=1, max_length=100)
    
    # Video information
    video_url: str = Field(..., min_length=1, max_length=500)
    video_title: str = Field(..., min_length=1, max_length=500)
    youtube_id: str = Field(..., min_length=1, max_length=100)
    
    # Processing status
    status: str = Field(default="processing")
    
    @field_validator("status")
    @classmethod
    def validate_video_status(cls, v: str) -> str:
        """Validate status value."""
        if v not in ["processing", "completed", "failed"]:
            raise ValueError(f"Invalid status: {v}. Must be one of: processing, completed, failed")
        return v
    
    # Timestamps
    created_at: datetime
    completed_at: Optional[datetime] = None
    failed_at: Optional[datetime] = None
    updated_at: datetime
    
    # Error information
    error_message: Optional[str] = Field(None, max_length=1000)
    
    # Highlights metadata
    highlights_count: int = Field(default=0, ge=0)
    highlights_summary: Optional[Dict[str, Any]] = None
    
    # Processing configuration
    custom_prompt: Optional[str] = Field(None, max_length=5000)
    styles_processed: List[str] = Field(default_factory=list)
    crop_mode: str = Field(default="none", max_length=50)
    target_aspect: str = Field(default="9:16", max_length=10)
    
    # Statistics
    clips_count: int = Field(default=0, ge=0)
    clips_by_style: Dict[str, int] = Field(default_factory=dict)
    
    # Storage references
    highlights_json_key: str = Field(..., min_length=1, max_length=500)
    
    # Metadata
    created_by: str = Field(..., min_length=1, max_length=100)
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "VideoMetadata":
        """Create from Firestore document dictionary."""
        return cls.model_validate(data)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to Firestore-compatible dictionary."""
        return self.model_dump(exclude_none=False)

