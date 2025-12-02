"""
Security module for Viral Clip AI.

Provides:
- Rate limiting (in-memory with Redis support)
- Input validation and sanitization
- Request ID tracking
- Security utilities
"""

from app.core.security.constants import (
    ALLOWED_VIDEO_HOSTS,
    MAX_DESCRIPTION_LENGTH,
    MAX_PROMPT_LENGTH,
    MAX_TITLE_LENGTH,
    MAX_URL_LENGTH,
    DEFAULT_RATE_LIMIT,
    DEFAULT_RATE_WINDOW,
    WEBSOCKET_RATE_LIMIT,
    WEBSOCKET_RATE_WINDOW,
    REQUEST_ID_HEADER,
)
from app.core.security.rate_limiting import (
    RateLimiter,
    get_api_rate_limiter,
    get_ws_rate_limiter,
    check_rate_limit,
    check_ws_rate_limit,
)
from app.core.security.validation import (
    ValidationError,
    validate_video_url,
    validate_prompt,
    validate_style,
    validate_video_id,
    validate_clip_name,
    sanitize_text,
)
from app.core.security.utils import (
    generate_request_id,
    get_request_id,
    hash_token,
    mask_sensitive_data,
    log_security_event,
)

__all__ = [
    # Constants
    "ALLOWED_VIDEO_HOSTS",
    "MAX_DESCRIPTION_LENGTH",
    "MAX_PROMPT_LENGTH",
    "MAX_TITLE_LENGTH",
    "MAX_URL_LENGTH",
    "DEFAULT_RATE_LIMIT",
    "DEFAULT_RATE_WINDOW",
    "WEBSOCKET_RATE_LIMIT",
    "WEBSOCKET_RATE_WINDOW",
    "REQUEST_ID_HEADER",
    # Rate limiting
    "RateLimiter",
    "get_api_rate_limiter",
    "get_ws_rate_limiter",
    "check_rate_limit",
    "check_ws_rate_limit",
    # Validation
    "ValidationError",
    "validate_video_url",
    "validate_prompt",
    "validate_style",
    "validate_video_id",
    "validate_clip_name",
    "sanitize_text",
    # Utils
    "generate_request_id",
    "get_request_id",
    "hash_token",
    "mask_sensitive_data",
    "log_security_event",
]

