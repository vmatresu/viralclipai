"""
Rate Limiting Middleware

Global rate limiting middleware based on IP address.
"""

from typing import Callable, Optional

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp

from app.config import logger
from app.core.security import get_api_rate_limiter, log_security_event


class RateLimitMiddleware(BaseHTTPMiddleware):
    """
    Global rate limiting middleware.
    
    Applies rate limits based on IP address.
    User-specific limits should be applied at the route level.
    """
    
    def __init__(self, app: ASGIApp, exclude_paths: Optional[set] = None):
        super().__init__(app)
        self.exclude_paths = exclude_paths or {"/health", "/healthz", "/ready", "/static"}
    
    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        # Skip rate limiting for excluded paths
        path = request.url.path
        if any(path.startswith(p) for p in self.exclude_paths):
            return await call_next(request)
        
        # Skip WebSocket upgrades (handled separately)
        if request.headers.get("upgrade", "").lower() == "websocket":
            return await call_next(request)
        
        limiter = get_api_rate_limiter()
        key = limiter.get_key_for_request(request)
        allowed, remaining, reset = limiter.is_allowed(key)
        
        if not allowed:
            log_security_event(
                "rate_limit_exceeded",
                request=request,
                details={"key": key, "reset_seconds": reset}
            )
            return Response(
                content='{"detail": "Rate limit exceeded. Please try again later."}',
                status_code=429,
                media_type="application/json",
                headers={
                    "Retry-After": str(reset),
                    "X-RateLimit-Limit": str(limiter._limit),
                    "X-RateLimit-Remaining": "0",
                    "X-RateLimit-Reset": str(reset),
                }
            )
        
        response = await call_next(request)
        
        # Add rate limit headers to response
        response.headers["X-RateLimit-Limit"] = str(limiter._limit)
        response.headers["X-RateLimit-Remaining"] = str(remaining)
        response.headers["X-RateLimit-Reset"] = str(reset)
        
        return response

