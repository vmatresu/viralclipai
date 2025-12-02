"""
Rate Limiting Module

Thread-safe in-memory rate limiter implementation.
For production with multiple workers, replace with Redis-based implementation.
"""

import time
from collections import defaultdict
from dataclasses import dataclass, field
from threading import Lock
from typing import Dict, Optional, Tuple

from fastapi import HTTPException, Request, status

from app.config import logger
from app.core.security.constants import (
    DEFAULT_RATE_LIMIT,
    DEFAULT_RATE_WINDOW,
    WEBSOCKET_RATE_LIMIT,
    WEBSOCKET_RATE_WINDOW,
)
from app.core.security.utils import log_security_event


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

