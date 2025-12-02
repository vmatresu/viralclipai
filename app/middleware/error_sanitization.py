"""
Error Sanitization Middleware

Sanitizes error responses to prevent information leakage.
"""

from typing import Callable

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp

from app.config import logger
from app.core.security.utils import get_request_id


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

