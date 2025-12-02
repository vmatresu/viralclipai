"""
Security Utilities

Request ID tracking, hashing, masking, and security event logging.
"""

import hashlib
import re
import secrets
from typing import Any, Dict, Optional

from fastapi import Request

from app.config import logger
from app.core.security.constants import REQUEST_ID_HEADER


def generate_request_id() -> str:
    """Generate a unique request ID."""
    return secrets.token_hex(16)


def get_request_id(request: Request) -> str:
    """Get or generate request ID from request."""
    request_id = request.headers.get(REQUEST_ID_HEADER)
    if request_id and len(request_id) <= 64 and re.match(r"^[a-zA-Z0-9_-]+$", request_id):
        return request_id
    return generate_request_id()


def hash_token(token: str) -> str:
    """
    Create a secure hash of a token for logging.
    
    Never log raw tokens - use this for audit trails.
    """
    return hashlib.sha256(token.encode()).hexdigest()[:16]


def mask_sensitive_data(
    data: Dict[str, Any],
    sensitive_keys: frozenset = frozenset({"token", "password", "secret", "key", "authorization"})
) -> Dict[str, Any]:
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
        # WebSocket objects don't have a method attribute
        log_data["method"] = getattr(request, "method", "WEBSOCKET")
        log_data["request_id"] = get_request_id(request)
    
    if details:
        log_data["details"] = mask_sensitive_data(details)
    
    log_func = getattr(logger, level, logger.warning)
    log_func("Security event: %s | %s", event_type, log_data)

