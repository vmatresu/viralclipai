"""
Pydantic models for request/response validation.

Provides type-safe, validated data structures for all API endpoints.
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field, field_validator, model_validator, ConfigDict

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
    )


# -----------------------------------------------------------------------------
# WebSocket Messages
# -----------------------------------------------------------------------------

class WSProcessRequest(BaseSchema):
    """WebSocket video processing request."""
    token: str = Field(..., min_length=1, description="Firebase auth token")
    url: str = Field(..., min_length=1, max_length=MAX_URL_LENGTH, description="Video URL")
    styles: Optional[List[str]] = Field(default=None, min_length=1, max_length=10, description="Clip styles")
    prompt: Optional[str] = Field(default=None, max_length=MAX_PROMPT_LENGTH, description="Custom prompt")
    crop_mode: str = Field(default="none", description="Crop mode: none, center, manual, intelligent")
    target_aspect: str = Field(default="9:16", description="Target aspect ratio for intelligent crop")


class WSReprocessRequest(BaseSchema):
    """WebSocket scene reprocessing request."""
    token: str = Field(..., min_length=1, description="Firebase auth token")
    video_id: str = Field(..., min_length=1, description="Video ID to reprocess scenes from")
    scene_ids: List[int] = Field(..., min_length=1, max_length=50, description="List of scene IDs to reprocess")
    styles: List[str] = Field(..., min_length=1, max_length=10, description="Styles to apply to each scene")
    crop_mode: str = Field(default="none", description="Crop mode: none, center, manual, intelligent")
    target_aspect: str = Field(default="9:16", description="Target aspect ratio for intelligent crop")
    
    @field_validator("styles")
    @classmethod
    def validate_styles_field(cls, v: List[str]) -> List[str]:
        if not v or len(v) == 0:
            raise ValueError("At least one style is required")
        # Validate each style
        validated_styles = []
        for style in v:
            validated_style = validate_style(style)
            if validated_style not in validated_styles:  # Remove duplicates
                validated_styles.append(validated_style)
        return validated_styles
    
    @field_validator("url")
    @classmethod
    def validate_url(cls, v: str) -> str:
        return validate_video_url(v)
    
    @field_validator("styles")
    @classmethod
    def validate_styles_field(cls, v: Optional[List[str]]) -> List[str]:
        if not v or len(v) == 0:
            return ["split"]  # Default to split if empty
        # Validate each style
        validated_styles = []
        for style in v:
            validated_style = validate_style(style)
            if validated_style not in validated_styles:  # Remove duplicates
                validated_styles.append(validated_style)
        return validated_styles if validated_styles else ["split"]
    
    @field_validator("prompt")
    @classmethod
    def validate_prompt_field(cls, v: Optional[str]) -> Optional[str]:
        return validate_prompt(v)
    
    @field_validator("crop_mode")
    @classmethod
    def validate_crop_mode_field(cls, v: str) -> str:
        allowed = ["none", "center", "manual", "intelligent"]
        if v not in allowed:
            raise ValueError(f"crop_mode must be one of: {allowed}")
        return v
    
    @field_validator("target_aspect")
    @classmethod
    def validate_target_aspect_field(cls, v: str) -> str:
        # Validate aspect ratio format (e.g., "9:16", "4:5", "1:1")
        import re
        if not re.match(r"^\d+:\d+$", v):
            raise ValueError("target_aspect must be in format 'W:H' (e.g., '9:16')")
        return v


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


class WSClipUploadedMessage(BaseSchema):
    """WebSocket clip upload notification message."""
    type: str = Field(default="clip_uploaded")
    videoId: str
    clipCount: int = Field(..., ge=0, description="Number of clips uploaded so far")
    totalClips: int = Field(..., ge=0, description="Total number of clips expected")


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


class UpdateVideoTitleRequest(BaseSchema):
    """Request to update video title."""
    title: str = Field(..., min_length=1, max_length=MAX_TITLE_LENGTH)
    
    @field_validator("title")
    @classmethod
    def sanitize_title(cls, v: str) -> str:
        return sanitize_text(v, MAX_TITLE_LENGTH)


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
    video_title: Optional[str] = None
    video_url: Optional[str] = None


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
    role: Optional[str] = None  # User role (e.g., "superadmin")


class SettingsUpdateResponse(BaseSchema):
    """Response after updating settings."""
    settings: Dict[str, Any]


class AdminPromptResponse(BaseSchema):
    """Response for admin prompt endpoints."""
    prompt: str


# -----------------------------------------------------------------------------
# Plan Management Schemas
# -----------------------------------------------------------------------------

class PlanLimitRequest(BaseSchema):
    """Request to update plan limits."""
    name: str = Field(..., min_length=1, max_length=100)
    value: int = Field(..., ge=0)
    description: Optional[str] = Field(None, max_length=500)


class PlanCreateRequest(BaseSchema):
    """Request to create a new plan."""
    id: str = Field(..., min_length=1, max_length=50)
    name: str = Field(..., min_length=1, max_length=100)
    description: Optional[str] = Field(None, max_length=500)
    price_monthly: Optional[float] = Field(None, ge=0)
    price_yearly: Optional[float] = Field(None, ge=0)
    limits: Dict[str, int] = Field(default_factory=dict)
    features: List[str] = Field(default_factory=list)
    status: str = Field(default="active")


class PlanUpdateRequest(BaseSchema):
    """Request to update an existing plan."""
    name: Optional[str] = Field(None, min_length=1, max_length=100)
    description: Optional[str] = Field(None, max_length=500)
    price_monthly: Optional[float] = Field(None, ge=0)
    price_yearly: Optional[float] = Field(None, ge=0)
    limits: Optional[Dict[str, int]] = None
    features: Optional[List[str]] = None
    status: Optional[str] = None


class PlanResponse(BaseSchema):
    """Response containing plan information."""
    id: str
    name: str
    description: Optional[str] = None
    price_monthly: Optional[float] = None
    price_yearly: Optional[float] = None
    status: str
    limits: Dict[str, int]
    features: List[str]
    created_at: datetime
    updated_at: datetime
    created_by: Optional[str] = None
    updated_by: Optional[str] = None


class PlansListResponse(BaseSchema):
    """Response containing list of plans."""
    plans: List[PlanResponse]


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


class DeleteClipResponse(BaseSchema):
    """Response after deleting a clip."""
    success: bool
    video_id: str
    clip_name: str
    message: Optional[str] = None
    files_deleted: Optional[int] = None


class UpdateVideoTitleResponse(BaseSchema):
    """Response after updating video title."""
    success: bool
    video_id: str
    title: str
    message: Optional[str] = None


class HighlightInfo(BaseSchema):
    """Information about a single highlight/scene."""
    id: int
    title: str
    start: str
    end: str
    duration: int
    hook_category: Optional[str] = None
    reason: Optional[str] = None
    description: Optional[str] = None


class HighlightsResponse(BaseSchema):
    """Response containing highlights for a video."""
    video_id: str
    video_url: Optional[str] = None
    video_title: Optional[str] = None
    highlights: List[HighlightInfo]


class ReprocessScenesRequest(BaseSchema):
    """Request to reprocess specific scenes with styles."""
    scene_ids: List[int] = Field(..., min_length=1, max_length=50, description="List of scene IDs to reprocess")
    styles: List[str] = Field(..., min_length=1, max_length=10, description="Styles to apply to each scene")
    
    @field_validator("styles")
    @classmethod
    def validate_styles_field(cls, v: List[str]) -> List[str]:
        if not v or len(v) == 0:
            raise ValueError("At least one style is required")
        # Validate each style
        validated_styles = []
        for style in v:
            validated_style = validate_style(style)
            if validated_style not in validated_styles:  # Remove duplicates
                validated_styles.append(validated_style)
        return validated_styles


class ReprocessScenesResponse(BaseSchema):
    """Response after initiating scene reprocessing."""
    success: bool
    video_id: str
    message: str
    job_id: Optional[str] = None
