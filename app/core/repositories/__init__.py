"""
Repository layer for Firestore data access.

This package provides clean, testable interfaces for data access following
Repository Pattern and Domain-Driven Design principles.
"""

from app.core.repositories.clips import ClipRepository, ClipRepositoryError
from app.core.repositories.videos import VideoRepository, VideoRepositoryError

__all__ = [
    "ClipRepository",
    "ClipRepositoryError",
    "VideoRepository",
    "VideoRepositoryError",
]

