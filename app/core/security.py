"""
Security module for Viral Clip AI.

Provides:
- Rate limiting (in-memory with Redis support)
- Input validation and sanitization
- Request ID tracking
- Security utilities
"""

import hashlib
import re
import secrets
import time
from collections import defaultdict
from dataclasses import dataclass, field
from functools import wraps
from threading import Lock
from typing import Any, Callable, Dict, Optional, Tuple
from urllib.parse import urlparse

from fastapi import HTTPException, Request, status

from app.config import logger

# -----------------------------------------------------------------------------
# Constants
# -----------------------------------------------------------------------------

# Allowed video URL patterns (YouTube, Vimeo, etc.)
ALLOWED_VIDEO_HOSTS = frozenset({
    "youtube.com",
    "www.youtube.com",
    "youtu.be",
    "m.youtube.com",
    "vimeo.com",
    "www.vimeo.com",
    "player.vimeo.com",
})

# Maximum lengths for user inputs
MAX_URL_LENGTH = 2048
MAX_PROMPT_LENGTH = 10000
MAX_TITLE_LENGTH = 500
MAX_DESCRIPTION_LENGTH = 5000

# Rate limiting defaults
DEFAULT_RATE_LIMIT = 60  # requests per window
DEFAULT_RATE_WINDOW = 60  # seconds
WEBSOCKET_RATE_LIMIT = 10  # connections per window
WEBSOCKET_RATE_WINDOW = 60  # seconds

# Request ID header
REQUEST_ID_HEADER = "X-Request-ID"


# -----------------------------------------------------------------------------
# Rate Limiter (Thread-safe in-memory implementation)
# -----------------------------------------------------------------------------

@dataclass
class RateLimitEntry:
    """Tracks rate limit state for a single key."""
    count: int = 0
    window_start: float = field(default_factory=time.time)


class RateLimiter:
    """
    Thread-safe in-memory rate limiter using sliding window.
    
    For production with multiple workers, replace with Redis-based implementation.
    """
    
    def __init__(self, limit: int = DEFAULT_RATE_LIMIT, window: int = DEFAULT_RATE_WINDOW):
        self._limit = limit
        self._window = window
        self._entries: Dict[str, RateLimitEntry] = defaultdict(RateLimitEntry)
        self._lock = Lock()
        self._cleanup_counter = 0
        self._cleanup_threshold = 1000  # Cleanup every N checks
    
    def _cleanup_expired(self) -> None:
        """Remove expired entries to prevent memory growth."""
        now = time.time()
        expired_keys = [
            key for key, entry in self._entries.items()
            if now - entry.window_start > self._window * 2
        ]
        for key in expired_keys:
            del self._entries[key]
    
    def is_allowed(self, key: str) -> Tuple[bool, int, int]:
        """
        Check if request is allowed under rate limit.
        
        Returns:
            Tuple of (allowed, remaining, reset_time)
        """
        now = time.time()
        
        with self._lock:
            # Periodic cleanup
            self._cleanup_counter += 1
            if self._cleanup_counter >= self._cleanup_threshold:
                self._cleanup_expired()
                self._cleanup_counter = 0
            
            entry = self._entries[key]
            
            # Reset window if expired
            if now - entry.window_start > self._window:
                entry.count = 0
                entry.window_start = now
            
            # Check limit
            if entry.count >= self._limit:
                reset_time = int(entry.window_start + self._window - now)
                return False, 0, max(reset_time, 1)
            
            # Increment and allow
            entry.count += 1
            remaining = self._limit - entry.count
            reset_time = int(entry.window_start + self._window - now)
            
            return True, remaining, max(reset_time, 1)
    
    def get_key_for_request(self, request: Request, user_id: Optional[str] = None) -> str:
        """Generate rate limit key from request."""
        if user_id:
            return f"user:{user_id}"
        
        # Fall back to IP-based limiting
        forwarded = request.headers.get("X-Forwarded-For")
        if forwarded:
            # Take first IP in chain (client IP)
            client_ip = forwarded.split(",")[0].strip()
        else:
            client_ip = request.client.host if request.client else "unknown"
        
        return f"ip:{client_ip}"


# Global rate limiters
_api_limiter = RateLimiter(limit=DEFAULT_RATE_LIMIT, window=DEFAULT_RATE_WINDOW)
_ws_limiter = RateLimiter(limit=WEBSOCKET_RATE_LIMIT, window=WEBSOCKET_RATE_WINDOW)


def get_api_rate_limiter() -> RateLimiter:
    """Get the global API rate limiter."""
    return _api_limiter


def get_ws_rate_limiter() -> RateLimiter:
    """Get the global WebSocket rate limiter."""
    return _ws_limiter


# -----------------------------------------------------------------------------
# Input Validation
# -----------------------------------------------------------------------------

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


# -----------------------------------------------------------------------------
# Request ID Tracking
# -----------------------------------------------------------------------------

def generate_request_id() -> str:
    """Generate a unique request ID."""
    return secrets.token_hex(16)


def get_request_id(request: Request) -> str:
    """Get or generate request ID from request."""
    request_id = request.headers.get(REQUEST_ID_HEADER)
    if request_id and len(request_id) <= 64 and re.match(r"^[a-zA-Z0-9_-]+$", request_id):
        return request_id
    return generate_request_id()


# -----------------------------------------------------------------------------
# Security Utilities
# -----------------------------------------------------------------------------

def hash_token(token: str) -> str:
    """
    Create a secure hash of a token for logging.
    
    Never log raw tokens - use this for audit trails.
    """
    return hashlib.sha256(token.encode()).hexdigest()[:16]


def mask_sensitive_data(data: Dict[str, Any], sensitive_keys: frozenset = frozenset({"token", "password", "secret", "key", "authorization"})) -> Dict[str, Any]:
    """
    Mask sensitive data in dictionaries for safe logging.
    """
    masked = {}
    for key, value in data.items():
        key_lower = key.lower()
        if any(s in key_lower for s in sensitive_keys):
            masked[key] = "[REDACTED]"
        elif isinstance(value, dict):
            masked[key] = mask_sensitive_data(value, sensitive_keys)
        else:
            masked[key] = value
    return masked


def log_security_event(
    event_type: str,
    request: Optional[Request] = None,
    user_id: Optional[str] = None,
    details: Optional[Dict[str, Any]] = None,
    level: str = "warning"
) -> None:
    """
    Log a security-relevant event with structured data.
    """
    log_data = {
        "security_event": event_type,
        "user_id": user_id,
    }
    
    if request:
        log_data["client_ip"] = request.headers.get("X-Forwarded-For", request.client.host if request.client else "unknown")
        log_data["path"] = str(request.url.path)
        log_data["method"] = request.method
        log_data["request_id"] = get_request_id(request)
    
    if details:
        log_data["details"] = mask_sensitive_data(details)
    
    log_func = getattr(logger, level, logger.warning)
    log_func("Security event: %s | %s", event_type, log_data)


# -----------------------------------------------------------------------------
# Dependency Injection Helpers
# -----------------------------------------------------------------------------

async def check_rate_limit(request: Request, user_id: Optional[str] = None) -> None:
    """
    FastAPI dependency to check rate limit.
    
    Raises HTTPException if rate limit exceeded.
    """
    limiter = get_api_rate_limiter()
    key = limiter.get_key_for_request(request, user_id)
    allowed, remaining, reset = limiter.is_allowed(key)
    
    # Set rate limit headers
    request.state.rate_limit_remaining = remaining
    request.state.rate_limit_reset = reset
    
    if not allowed:
        log_security_event(
            "rate_limit_exceeded",
            request=request,
            user_id=user_id,
            details={"key": key, "reset_seconds": reset}
        )
        raise HTTPException(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            detail=f"Rate limit exceeded. Try again in {reset} seconds.",
            headers={
                "Retry-After": str(reset),
                "X-RateLimit-Remaining": "0",
                "X-RateLimit-Reset": str(reset),
            }
        )


async def check_ws_rate_limit(request: Request, user_id: Optional[str] = None) -> bool:
    """
    Check WebSocket connection rate limit.
    
    Returns True if allowed, False if rate limited.
    """
    limiter = get_ws_rate_limiter()
    key = limiter.get_key_for_request(request, user_id)
    allowed, _, reset = limiter.is_allowed(key)
    
    if not allowed:
        log_security_event(
            "ws_rate_limit_exceeded",
            request=request,
            user_id=user_id,
            details={"key": key, "reset_seconds": reset}
        )
    
    return allowed
