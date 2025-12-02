"""
Request ID Middleware

Injects a unique request ID into each request for tracing.
"""

from typing import Callable

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware

from app.core.security.utils import get_request_id
from app.core.security.constants import REQUEST_ID_HEADER


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

