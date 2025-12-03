"""
Repository exceptions for clean error handling.

Following best practices for exception hierarchy and error propagation.
"""


class RepositoryError(Exception):
    """Base exception for all repository errors."""
    pass


class ClipRepositoryError(RepositoryError):
    """Exception raised by ClipRepository operations."""
    pass


class VideoRepositoryError(RepositoryError):
    """Exception raised by VideoRepository operations."""
    pass


class NotFoundError(RepositoryError):
    """Raised when a requested resource is not found."""
    pass


class ValidationError(RepositoryError):
    """Raised when input validation fails."""
    pass


class ConflictError(RepositoryError):
    """Raised when a conflict occurs (e.g., duplicate creation)."""
    pass

