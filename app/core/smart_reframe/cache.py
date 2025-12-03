"""
Shot detection cache for performance optimization.

This module provides caching of shot detection results to avoid
re-analyzing the same video multiple times when processing multiple clips.
"""

import hashlib
import logging
import pickle
from pathlib import Path
from typing import Optional

from app.core.smart_reframe.models import Shot
from app.core.smart_reframe.shot_detector import ShotDetector
from app.core.smart_reframe.config import IntelligentCropConfig

logger = logging.getLogger(__name__)


class ShotDetectionCache:
    """
    Cache manager for shot detection results.
    
    Caches shot boundaries per video file to avoid re-detection
    when processing multiple clips from the same video.
    """

    def __init__(self, cache_dir: Optional[Path] = None):
        """
        Initialize the cache.
        
        Args:
            cache_dir: Directory for cache files. Defaults to system temp.
        """
        if cache_dir is None:
            import tempfile
            cache_dir = Path(tempfile.gettempdir()) / "viralclipai_shot_cache"
        
        self.cache_dir = Path(cache_dir)
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        
        # In-memory cache for active session
        self._memory_cache: dict[str, list[Shot]] = {}

    def _get_cache_key(self, video_path: str, config: IntelligentCropConfig) -> Optional[str]:
        """
        Generate a cache key for a video and config.
        
        Args:
            video_path: Path to video file.
            config: Configuration used for detection.
            
        Returns:
            Cache key string or None if video doesn't exist.
        """
        # Include video path, size, mtime, and relevant config params
        video_file = Path(video_path)
        if not video_file.exists():
            return None
        
        stat = video_file.stat()
        key_data = (
            str(video_path),
            stat.st_size,
            stat.st_mtime,
            config.fps_sample,
            config.shot_threshold,
            config.min_shot_duration,
        )
        
        key_str = "|".join(str(x) for x in key_data)
        return hashlib.sha256(key_str.encode()).hexdigest()

    def _get_cache_path(self, cache_key: str) -> Path:
        """Get the cache file path for a key."""
        return self.cache_dir / f"{cache_key}.pkl"

    def get_shots(
        self,
        video_path: str,
        config: IntelligentCropConfig,
        time_range: Optional[tuple[float, float]] = None,
    ) -> Optional[list[Shot]]:
        """
        Get cached shots or None if not cached.
        
        Args:
            video_path: Path to video file.
            config: Configuration used for detection.
            time_range: Optional time range (not used for caching).
            
        Returns:
            List of shots if cached, None otherwise.
        """
        # Check memory cache first
        cache_key = self._get_cache_key(video_path, config)
        if cache_key is None:
            return None
            
        if cache_key in self._memory_cache:
            shots = self._memory_cache[cache_key]
            logger.debug(f"Shot cache hit (memory): {video_path}")
            return shots
        
        # Check disk cache
        cache_path = self._get_cache_path(cache_key)
        if cache_path.exists():
            try:
                with open(cache_path, "rb") as f:
                    shots = pickle.load(f)
                # Store in memory cache
                self._memory_cache[cache_key] = shots
                logger.debug(f"Shot cache hit (disk): {video_path}")
                return shots
            except Exception as e:
                logger.warning(f"Failed to load shot cache: {e}")
                # Remove corrupted cache
                cache_path.unlink(missing_ok=True)
        
        return None

    def store_shots(
        self,
        video_path: str,
        config: IntelligentCropConfig,
        shots: list[Shot],
    ) -> None:
        """
        Store shots in cache.
        
        Args:
            video_path: Path to video file.
            config: Configuration used for detection.
            shots: List of detected shots.
        """
        cache_key = self._get_cache_key(video_path, config)
        if cache_key is None:
            return
        
        # Store in memory cache
        self._memory_cache[cache_key] = shots
        
        # Store on disk
        cache_path = self._get_cache_path(cache_key)
        try:
            with open(cache_path, "wb") as f:
                pickle.dump(shots, f)
            logger.debug(f"Stored shot cache: {video_path}")
        except Exception as e:
            logger.warning(f"Failed to store shot cache: {e}")

    def clear_cache(self, video_path: Optional[str] = None) -> None:
        """
        Clear cache entries.
        
        Args:
            video_path: If provided, clear only for this video.
                        Otherwise, clear all caches.
        """
        if video_path:
            cache_key = self._get_cache_key(video_path, IntelligentCropConfig())
            if cache_key:
                self._memory_cache.pop(cache_key, None)
                cache_path = self._get_cache_path(cache_key)
                cache_path.unlink(missing_ok=True)
        else:
            self._memory_cache.clear()
            for cache_file in self.cache_dir.glob("*.pkl"):
                cache_file.unlink(missing_ok=True)


# Global cache instance
_global_cache: Optional[ShotDetectionCache] = None


def get_shot_cache() -> ShotDetectionCache:
    """Get the global shot detection cache instance."""
    global _global_cache
    if _global_cache is None:
        _global_cache = ShotDetectionCache()
    return _global_cache


def detect_shots_cached(
    video_path: str,
    config: IntelligentCropConfig,
    time_range: Optional[tuple[float, float]] = None,
    cache: Optional[ShotDetectionCache] = None,
) -> list[Shot]:
    """
    Detect shots with caching support.
    
    Args:
        video_path: Path to video file.
        config: Configuration for detection.
        time_range: Optional time range to analyze.
        cache: Optional cache instance. Uses global if not provided.
        
    Returns:
        List of detected shots.
    """
    if cache is None:
        cache = get_shot_cache()
    
    # Try to get from cache (only if no time_range, as cache is for full video)
    if time_range is None:
        cached_shots = cache.get_shots(video_path, config)
        if cached_shots is not None:
            return cached_shots
    
    # Detect shots
    detector = ShotDetector(config)
    shots = detector.detect_shots(video_path, time_range)
    
    # Store in cache (only if no time_range)
    if time_range is None:
        cache.store_shots(video_path, config, shots)
    
    return shots

