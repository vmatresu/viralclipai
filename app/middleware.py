"""
Security middleware stack for Viral Clip AI.

Provides:
- Request ID injection
- Rate limiting
- Security headers (CSP, HSTS, etc.)
- Request/response logging
- Error sanitization
"""

import time
from typing import Callable, Optional

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp

from app.config import logger
from app.core.security import (
    generate_request_id,
    get_api_rate_limiter,
    get_request_id,
    log_security_event,
    REQUEST_ID_HEADER,
)


# -----------------------------------------------------------------------------
# Request ID Middleware
# -----------------------------------------------------------------------------

class RequestIDMiddleware(BaseHTTPMiddleware):
    """
    Injects a unique request ID into each request for tracing.
    
    - Uses existing X-Request-ID header if valid
    - Generates new ID otherwise
    - Adds ID to response headers
    """
    
    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        request_id = get_request_id(request)
        request.state.request_id = request_id
        
        response = await call_next(request)
        response.headers[REQUEST_ID_HEADER] = request_id
        
        return response


# -----------------------------------------------------------------------------
# Rate Limiting Middleware
# -----------------------------------------------------------------------------

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


# -----------------------------------------------------------------------------
# Security Headers Middleware
# -----------------------------------------------------------------------------

class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    """
    Adds comprehensive security headers to all responses.
    
    Headers:
    - X-Frame-Options: Prevents clickjacking
    - X-Content-Type-Options: Prevents MIME sniffing
    - X-XSS-Protection: Legacy XSS protection
    - Referrer-Policy: Controls referrer information
    - Strict-Transport-Security: Enforces HTTPS
    - Content-Security-Policy: Restricts resource loading
    - Permissions-Policy: Restricts browser features
    - Cache-Control: Prevents caching of sensitive data
    """
    
    def __init__(
        self,
        app: ASGIApp,
        csp_policy: Optional[str] = None,
        hsts_max_age: int = 31536000,
        include_subdomains: bool = True,
        hsts_preload: bool = True,
    ):
        super().__init__(app)
        
        # Build HSTS header
        hsts_parts = [f"max-age={hsts_max_age}"]
        if include_subdomains:
            hsts_parts.append("includeSubDomains")
        if hsts_preload:
            hsts_parts.append("preload")
        self.hsts_header = "; ".join(hsts_parts)
        
        # Default CSP - restrictive but allows necessary resources
        self.csp_policy = csp_policy or "; ".join([
            "default-src 'self'",
            "script-src 'self' 'unsafe-inline'",  # Adjust based on frontend needs
            "style-src 'self' 'unsafe-inline'",
            "img-src 'self' data: https:",
            "font-src 'self' data:",
            "connect-src 'self' wss: https:",
            "media-src 'self' https:",
            "frame-ancestors 'none'",
            "base-uri 'self'",
            "form-action 'self'",
            "upgrade-insecure-requests",
        ])
        
        # Permissions Policy (formerly Feature-Policy)
        self.permissions_policy = ", ".join([
            "accelerometer=()",
            "camera=()",
            "geolocation=()",
            "gyroscope=()",
            "magnetometer=()",
            "microphone=()",
            "payment=()",
            "usb=()",
        ])
    
    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        response = await call_next(request)
        
        # Core security headers
        response.headers.setdefault("X-Frame-Options", "DENY")
        response.headers.setdefault("X-Content-Type-Options", "nosniff")
        response.headers.setdefault("X-XSS-Protection", "1; mode=block")
        response.headers.setdefault("Referrer-Policy", "strict-origin-when-cross-origin")
        
        # HSTS - only set for HTTPS or when behind proxy
        response.headers.setdefault("Strict-Transport-Security", self.hsts_header)
        
        # CSP - skip for static files to avoid breaking them
        if not request.url.path.startswith("/static"):
            response.headers.setdefault("Content-Security-Policy", self.csp_policy)
        
        # Permissions Policy
        response.headers.setdefault("Permissions-Policy", self.permissions_policy)
        
        # Prevent caching of API responses
        if request.url.path.startswith("/api"):
            response.headers.setdefault("Cache-Control", "no-store, no-cache, must-revalidate, private")
            response.headers.setdefault("Pragma", "no-cache")
            response.headers.setdefault("Expires", "0")
        
        return response


# -----------------------------------------------------------------------------
# Request Logging Middleware
# -----------------------------------------------------------------------------

class RequestLoggingMiddleware(BaseHTTPMiddleware):
    """
    Logs request/response information for observability.
    
    Logs:
    - Request method, path, client IP
    - Response status code
    - Request duration
    - Request ID for correlation
    """
    
    def __init__(self, app: ASGIApp, log_body: bool = False, exclude_paths: Optional[set] = None):
        super().__init__(app)
        self.log_body = log_body
        self.exclude_paths = exclude_paths or {"/health", "/healthz", "/ready"}
    
    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        # Skip logging for health checks
        if request.url.path in self.exclude_paths:
            return await call_next(request)
        
        start_time = time.time()
        request_id = getattr(request.state, "request_id", "unknown")
        
        # Get client IP
        forwarded = request.headers.get("X-Forwarded-For")
        client_ip = forwarded.split(",")[0].strip() if forwarded else (request.client.host if request.client else "unknown")
        
        # Log request
        logger.info(
            "Request: %s %s | client=%s | request_id=%s",
            request.method,
            request.url.path,
            client_ip,
            request_id,
        )
        
        try:
            response = await call_next(request)
            duration_ms = (time.time() - start_time) * 1000
            
            # Log response
            logger.info(
                "Response: %s %s | status=%d | duration=%.2fms | request_id=%s",
                request.method,
                request.url.path,
                response.status_code,
                duration_ms,
                request_id,
            )
            
            return response
            
        except Exception as exc:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "Request failed: %s %s | error=%s | duration=%.2fms | request_id=%s",
                request.method,
                request.url.path,
                str(exc),
                duration_ms,
                request_id,
            )
            raise


# -----------------------------------------------------------------------------
# Error Sanitization Middleware
# -----------------------------------------------------------------------------

class ErrorSanitizationMiddleware(BaseHTTPMiddleware):
    """
    Sanitizes error responses to prevent information leakage.
    
    In production:
    - Removes stack traces from 500 errors
    - Provides generic error messages
    - Logs full errors server-side
    """
    
    def __init__(self, app: ASGIApp, debug: bool = False):
        super().__init__(app)
        self.debug = debug
    
    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        try:
            response = await call_next(request)
            
            # Sanitize 500 errors in production
            if response.status_code >= 500 and not self.debug:
                request_id = getattr(request.state, "request_id", "unknown")
                return Response(
                    content=f'{{"detail": "Internal server error", "request_id": "{request_id}"}}',
                    status_code=500,
                    media_type="application/json",
                )
            
            return response
            
        except Exception as exc:
            request_id = getattr(request.state, "request_id", "unknown")
            logger.exception("Unhandled exception in request %s: %s", request_id, exc)
            
            if self.debug:
                raise
            
            return Response(
                content=f'{{"detail": "Internal server error", "request_id": "{request_id}"}}',
                status_code=500,
                media_type="application/json",
            )
