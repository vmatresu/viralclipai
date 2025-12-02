"""
Pydantic models for request/response validation.

Provides type-safe, validated data structures for all API endpoints.
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field, field_validator, ConfigDict

from app.core.security import (
    MAX_DESCRIPTION_LENGTH,
    MAX_PROMPT_LENGTH,
    MAX_TITLE_LENGTH,
    MAX_URL_LENGTH,
    sanitize_text,
    validate_prompt,
    validate_style,
    validate_video_url,
    validate_video_id,
    ValidationError,
)


# -----------------------------------------------------------------------------
# Base Models
# -----------------------------------------------------------------------------

class BaseSchema(BaseModel):
    """Base schema with common configuration."""
    model_config = ConfigDict(
        str_strip_whitespace=True,
        str_min_length=0,
        extra="forbid",  # Reject unknown fields
    )


# -----------------------------------------------------------------------------
# WebSocket Messages
# -----------------------------------------------------------------------------

class WSProcessRequest(BaseSchema):
    """WebSocket video processing request."""
    token: str = Field(..., min_length=1, description="Firebase auth token")
    url: str = Field(..., min_length=1, max_length=MAX_URL_LENGTH, description="Video URL")
    style: str = Field(default="split", description="Clip style")
    prompt: Optional[str] = Field(default=None, max_length=MAX_PROMPT_LENGTH, description="Custom prompt")
    
    @field_validator("url")
    @classmethod
    def validate_url(cls, v: str) -> str:
        return validate_video_url(v)
    
    @field_validator("style")
    @classmethod
    def validate_style_field(cls, v: str) -> str:
        return validate_style(v)
    
    @field_validator("prompt")
    @classmethod
    def validate_prompt_field(cls, v: Optional[str]) -> Optional[str]:
        return validate_prompt(v)


class WSMessage(BaseSchema):
    """Generic WebSocket message."""
    model_config = ConfigDict(extra="allow")  # Allow extra fields for flexibility
    
    type: str = Field(..., description="Message type")
    message: Optional[str] = Field(default=None, description="Message content")


class WSErrorMessage(BaseSchema):
    """WebSocket error message."""
    type: str = Field(default="error")
    message: str
    details: Optional[str] = None


class WSProgressMessage(BaseSchema):
    """WebSocket progress message."""
    type: str = Field(default="progress")
    value: int = Field(..., ge=0, le=100)


class WSDoneMessage(BaseSchema):
    """WebSocket completion message."""
    type: str = Field(default="done")
    videoId: str


# -----------------------------------------------------------------------------
# API Request Models
# -----------------------------------------------------------------------------

class SettingsUpdateRequest(BaseSchema):
    """Request to update user settings."""
    settings: Dict[str, Any] = Field(default_factory=dict)
    
    @field_validator("settings")
    @classmethod
    def validate_settings(cls, v: Dict[str, Any]) -> Dict[str, Any]:
        # Limit settings size to prevent abuse
        if len(str(v)) > 10000:
            raise ValueError("Settings payload too large")
        return v


class TikTokPublishRequest(BaseSchema):
    """Request to publish clip to TikTok."""
    title: Optional[str] = Field(default=None, max_length=MAX_TITLE_LENGTH)
    description: Optional[str] = Field(default=None, max_length=MAX_DESCRIPTION_LENGTH)
    
    @field_validator("title")
    @classmethod
    def sanitize_title(cls, v: Optional[str]) -> Optional[str]:
        if v:
            return sanitize_text(v, MAX_TITLE_LENGTH)
        return v
    
    @field_validator("description")
    @classmethod
    def sanitize_description(cls, v: Optional[str]) -> Optional[str]:
        if v:
            return sanitize_text(v, MAX_DESCRIPTION_LENGTH)
        return v


class AdminPromptRequest(BaseSchema):
    """Request to update admin prompt."""
    prompt: str = Field(..., max_length=MAX_PROMPT_LENGTH)
    
    @field_validator("prompt")
    @classmethod
    def sanitize_prompt(cls, v: str) -> str:
        return sanitize_text(v, MAX_PROMPT_LENGTH)


# -----------------------------------------------------------------------------
# API Response Models
# -----------------------------------------------------------------------------

class ClipInfo(BaseSchema):
    """Information about a single clip."""
    model_config = ConfigDict(extra="allow")
    
    name: str
    title: str
    description: str = ""
    url: str
    thumbnail: Optional[str] = None
    size: str = ""


class VideoInfoResponse(BaseSchema):
    """Response for video info endpoint."""
    id: str
    clips: List[ClipInfo]
    custom_prompt: Optional[str] = None


class VideoSummary(BaseSchema):
    """Summary of a video in user's library."""
    model_config = ConfigDict(extra="allow")
    
    id: str
    video_id: Optional[str] = None
    video_url: Optional[str] = None
    video_title: Optional[str] = None
    clips_count: int = 0
    created_at: Optional[datetime] = None


class UserVideosResponse(BaseSchema):
    """Response for user videos list."""
    videos: List[VideoSummary]


class UserSettingsResponse(BaseSchema):
    """Response for user settings."""
    settings: Dict[str, Any]
    plan: str
    max_clips_per_month: int
    clips_used_this_month: int


class SettingsUpdateResponse(BaseSchema):
    """Response after updating settings."""
    settings: Dict[str, Any]


class AdminPromptResponse(BaseSchema):
    """Response for admin prompt endpoints."""
    prompt: str


class TikTokPublishResponse(BaseSchema):
    """Response after publishing to TikTok."""
    model_config = ConfigDict(extra="allow")
    
    success: bool = True
    message: Optional[str] = None


class ErrorResponse(BaseSchema):
    """Standard error response."""
    detail: str
    code: Optional[str] = None


class HealthResponse(BaseSchema):
    """Health check response."""
    status: str = "healthy"
    version: str
    timestamp: datetime = Field(default_factory=datetime.utcnow)


# -----------------------------------------------------------------------------
# Delete Request/Response Models
# -----------------------------------------------------------------------------

class BulkDeleteVideosRequest(BaseSchema):
    """Request to delete multiple videos."""
    video_ids: List[str] = Field(..., min_length=1, max_length=100, description="List of video IDs to delete")
    
    @field_validator("video_ids")
    @classmethod
    def validate_video_ids(cls, v: List[str]) -> List[str]:
        """Validate and sanitize video IDs."""
        validated_ids = []
        for video_id in v:
            try:
                validated_id = validate_video_id(video_id)
                validated_ids.append(validated_id)
            except ValidationError:
                # Skip invalid IDs - we'll report them in the response
                pass
        
        if not validated_ids:
            raise ValueError("At least one valid video ID is required")
        
        # Remove duplicates while preserving order
        seen = set()
        unique_ids = []
        for video_id in validated_ids:
            if video_id not in seen:
                seen.add(video_id)
                unique_ids.append(video_id)
        
        return unique_ids


class DeleteVideoResponse(BaseSchema):
    """Response after deleting a video."""
    success: bool
    video_id: str
    message: Optional[str] = None
    files_deleted: Optional[int] = None


class BulkDeleteVideosResponse(BaseSchema):
    """Response after bulk deleting videos."""
    success: bool
    deleted_count: int
    failed_count: int
    results: Dict[str, Dict[str, Any]] = Field(
        default_factory=dict,
        description="Detailed results per video_id with success status and optional error"
    )
