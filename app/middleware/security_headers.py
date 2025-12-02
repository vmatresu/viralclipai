"""
Security Headers Middleware

Adds comprehensive security headers to all responses.
"""

from typing import Callable, Optional

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp


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

