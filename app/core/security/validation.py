"""
Input Validation Module

Validates and sanitizes user inputs to prevent security issues.
"""

import re
from typing import Optional
from urllib.parse import urlparse

from app.core.security.constants import (
    ALLOWED_VIDEO_HOSTS,
    MAX_DESCRIPTION_LENGTH,
    MAX_PROMPT_LENGTH,
    MAX_TITLE_LENGTH,
    MAX_URL_LENGTH,
)


class ValidationError(Exception):
    """Raised when input validation fails."""
    def __init__(self, message: str, field: Optional[str] = None):
        self.message = message
        self.field = field
        super().__init__(message)


def validate_video_url(url: str) -> str:
    """
    Validate and sanitize video URL.
    
    Raises:
        ValidationError: If URL is invalid or from disallowed host.
    
    Returns:
        Sanitized URL string.
    """
    if not url or not isinstance(url, str):
        raise ValidationError("URL is required", field="url")
    
    url = url.strip()
    
    if len(url) > MAX_URL_LENGTH:
        raise ValidationError(f"URL exceeds maximum length of {MAX_URL_LENGTH}", field="url")
    
    # Basic URL validation
    try:
        parsed = urlparse(url)
    except Exception:
        raise ValidationError("Invalid URL format", field="url")
    
    if parsed.scheme not in ("http", "https"):
        raise ValidationError("URL must use HTTP or HTTPS", field="url")
    
    if not parsed.netloc:
        raise ValidationError("Invalid URL: missing host", field="url")
    
    # Check against allowed hosts
    host = parsed.netloc.lower()
    if host not in ALLOWED_VIDEO_HOSTS:
        raise ValidationError(
            f"Unsupported video host. Allowed: {', '.join(sorted(ALLOWED_VIDEO_HOSTS))}",
            field="url"
        )
    
    return url


def validate_prompt(prompt: Optional[str]) -> Optional[str]:
    """
    Validate and sanitize custom prompt.
    
    Returns:
        Sanitized prompt or None.
    """
    if not prompt:
        return None
    
    if not isinstance(prompt, str):
        raise ValidationError("Prompt must be a string", field="prompt")
    
    prompt = prompt.strip()
    
    if not prompt:
        return None
    
    if len(prompt) > MAX_PROMPT_LENGTH:
        raise ValidationError(f"Prompt exceeds maximum length of {MAX_PROMPT_LENGTH}", field="prompt")
    
    return prompt


def validate_style(style: str) -> str:
    """Validate clip style parameter."""
    ALLOWED_STYLES = {"split", "vertical", "horizontal", "all"}
    
    if not style or not isinstance(style, str):
        return "split"  # Default
    
    style = style.strip().lower()
    
    if style not in ALLOWED_STYLES:
        raise ValidationError(
            f"Invalid style. Allowed: {', '.join(sorted(ALLOWED_STYLES))}",
            field="style"
        )
    
    return style


def validate_video_id(video_id: str) -> str:
    """
    Validate video ID format (prevents path traversal).
    
    Video IDs should be alphanumeric with limited special chars.
    """
    if not video_id or not isinstance(video_id, str):
        raise ValidationError("Video ID is required", field="video_id")
    
    video_id = video_id.strip()
    
    # Allow alphanumeric, hyphens, underscores only
    if not re.match(r"^[a-zA-Z0-9_-]+$", video_id):
        raise ValidationError("Invalid video ID format", field="video_id")
    
    if len(video_id) > 100:
        raise ValidationError("Video ID too long", field="video_id")
    
    return video_id


def validate_clip_name(clip_name: str) -> str:
    """Validate clip name format (prevents path traversal)."""
    if not clip_name or not isinstance(clip_name, str):
        raise ValidationError("Clip name is required", field="clip_name")
    
    clip_name = clip_name.strip()
    
    # Allow alphanumeric, hyphens, underscores, dots only
    if not re.match(r"^[a-zA-Z0-9_.-]+$", clip_name):
        raise ValidationError("Invalid clip name format", field="clip_name")
    
    if len(clip_name) > 200:
        raise ValidationError("Clip name too long", field="clip_name")
    
    # Must end with .mp4
    if not clip_name.lower().endswith(".mp4"):
        raise ValidationError("Clip name must end with .mp4", field="clip_name")
    
    return clip_name


def sanitize_text(text: str, max_length: int = 1000) -> str:
    """
    Sanitize text input by removing potentially dangerous characters.
    
    Preserves most Unicode for internationalization.
    """
    if not text:
        return ""
    
    # Remove null bytes and control characters (except newlines/tabs)
    text = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text)
    
    # Truncate
    if len(text) > max_length:
        text = text[:max_length]
    
    return text.strip()

