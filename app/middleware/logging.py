"""
Request Logging Middleware

Logs request/response information for observability.
"""

import time
from typing import Callable, Optional

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp

from app.config import logger
from app.core.security.utils import get_request_id


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

