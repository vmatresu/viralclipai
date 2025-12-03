"""
Simple TTL cache for video info responses.

Caches video information to avoid regenerating presigned URLs
on every request, since videos don't change once generated.
"""

import time
from typing import Any, Dict, Optional
from threading import Lock

from app.config import logger


class TTLCache:
    """Simple thread-safe TTL cache."""
    
    def __init__(self, ttl_seconds: int = 1800):
        """
        Initialize cache with TTL.
        
        Args:
            ttl_seconds: Time to live in seconds (default: 1800 = 30 minutes)
        """
        self._cache: Dict[str, tuple[Any, float]] = {}
        self._lock = Lock()
        self._ttl_seconds = ttl_seconds
    
    def get(self, key: str) -> Optional[Any]:
        """Get value from cache if it exists and hasn't expired."""
        with self._lock:
            if key not in self._cache:
                return None
            
            value, timestamp = self._cache[key]
            age = time.time() - timestamp
            
            if age >= self._ttl_seconds:
                # Expired, remove it
                del self._cache[key]
                logger.debug(f"Cache expired for key: {key}")
                return None
            
            logger.debug(f"Cache hit for key: {key} (age: {age:.1f}s)")
            return value
    
    def set(self, key: str, value: Any) -> None:
        """Store value in cache with current timestamp."""
        with self._lock:
            self._cache[key] = (value, time.time())
            logger.debug(f"Cache set for key: {key}")
    
    def invalidate(self, key: Optional[str] = None) -> None:
        """
        Invalidate cache entry(ies).
        
        Args:
            key: If provided, invalidate only this key. Otherwise, clear all.
        """
        with self._lock:
            if key is None:
                self._cache.clear()
                logger.debug("Cache cleared")
            elif key in self._cache:
                del self._cache[key]
                logger.debug(f"Cache invalidated for key: {key}")
    
    def clear(self) -> None:
        """Clear all cache entries."""
        self.invalidate()


# Global cache instance for video info
# TTL of 30 minutes (1800 seconds) - presigned URLs expire after 1 hour,
# so this ensures URLs are still valid when served from cache
_video_info_cache = TTLCache(ttl_seconds=1800)


def get_video_info_cache() -> TTLCache:
    """Get the global video info cache instance."""
    return _video_info_cache

