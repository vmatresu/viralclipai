"""
Video metadata repository for Firestore.

This module provides a production-ready repository for managing video metadata
with proper error handling, transactions, and performance optimizations.
"""

import logging
from contextlib import contextmanager
from datetime import datetime, timezone
from typing import Any, Dict, Iterator, List, Optional

from google.cloud import firestore
from google.cloud.firestore_v1 import Transaction

from app.core.firebase_client import get_firestore_client
from app.core.repositories.clips import ClipRepository
from app.core.repositories.exceptions import (
    NotFoundError,
    ValidationError,
    VideoRepositoryError,
)
from app.core.repositories.models import VideoMetadata

logger = logging.getLogger(__name__)


class VideoRepository:
    """
    Repository for managing video metadata in Firestore.
    
    Follows Repository Pattern with:
    - Type-safe operations using Pydantic models
    - Transaction support for consistency
    - Comprehensive error handling
    - Security validation
    - Statistics aggregation
    """

    def __init__(self, user_id: str):
        """
        Initialize video repository.
        
        Args:
            user_id: User ID (validated)
            
        Raises:
            ValidationError: If user_id is invalid
        """
        if not user_id or not isinstance(user_id, str) or len(user_id) > 100:
            raise ValidationError("Invalid user_id")
        
        self.user_id = user_id
        self.db = get_firestore_client()
        self.videos_collection = (
            self.db.collection("users")
            .document(user_id)
            .collection("videos")
        )

    @contextmanager
    def transaction(self) -> Iterator[Transaction]:
        """
        Context manager for Firestore transactions.
        
        Usage:
            with repo.transaction() as transaction:
                repo.create_video(..., transaction=transaction)
                transaction.commit()
        """
        transaction = self.db.transaction()
        try:
            yield transaction
        except Exception as e:
            logger.error(f"Transaction failed: {e}", exc_info=True)
            raise VideoRepositoryError(f"Transaction failed: {e}") from e

    def create_or_update_video(
        self,
        video_metadata: VideoMetadata,
        transaction: Optional[Transaction] = None,
    ) -> VideoMetadata:
        """
        Create or update video metadata.
        
        Args:
            video_metadata: VideoMetadata model instance
            transaction: Optional Firestore transaction
            
        Returns:
            Created/updated VideoMetadata instance
            
        Raises:
            ValidationError: If metadata is invalid
            VideoRepositoryError: If operation fails
        """
        try:
            # Validate metadata
            if video_metadata.user_id != self.user_id:
                raise ValidationError("user_id mismatch")
            
            doc_ref = self.videos_collection.document(video_metadata.video_id)
            video_data = video_metadata.to_dict()
            
            # Use set with merge=True to support updates
            if transaction:
                transaction.set(doc_ref, video_data, merge=True)
            else:
                doc_ref.set(video_data, merge=True)
            
            logger.debug(f"Created/updated video metadata: {video_metadata.video_id}")
            return video_metadata
            
        except ValidationError:
            raise
        except Exception as e:
            logger.error(
                f"Failed to create/update video {video_metadata.video_id}: {e}",
                exc_info=True
            )
            raise VideoRepositoryError(f"Failed to create/update video: {e}") from e

    def update_video_status(
        self,
        video_id: str,
        status: str,
        clips_count: Optional[int] = None,
        clips_by_style: Optional[Dict[str, int]] = None,
        error_message: Optional[str] = None,
        transaction: Optional[Transaction] = None,
    ) -> bool:
        """
        Update video status and statistics.
        
        Args:
            video_id: Video ID
            status: New status ("processing", "completed", "failed")
            clips_count: Optional total clips count
            clips_by_style: Optional clips count by style
            error_message: Optional error message for failed status
            transaction: Optional Firestore transaction
            
        Returns:
            True if updated, False if not found
            
        Raises:
            ValidationError: If status is invalid
            VideoRepositoryError: If update fails
        """
        if status not in ["processing", "completed", "failed"]:
            raise ValidationError(f"Invalid status: {status}")
        
        try:
            doc_ref = self.videos_collection.document(video_id)
            
            if transaction:
                doc = doc_ref.get(transaction=transaction)
            else:
                doc = doc_ref.get()
            
            if not doc.exists:
                logger.warning(f"Video {video_id} not found for update")
                return False
            
            update_data: Dict[str, Any] = {
                "status": status,
                "updated_at": datetime.now(timezone.utc),
            }
            
            if status == "completed":
                update_data["completed_at"] = datetime.now(timezone.utc)
            elif status == "failed":
                update_data["failed_at"] = datetime.now(timezone.utc)
                if error_message:
                    update_data["error_message"] = error_message[:1000]  # Limit length
            
            if clips_count is not None:
                if clips_count < 0:
                    raise ValidationError("clips_count must be non-negative")
                update_data["clips_count"] = clips_count
            
            if clips_by_style is not None:
                # Validate clips_by_style
                for style, count in clips_by_style.items():
                    if not isinstance(style, str) or not isinstance(count, int) or count < 0:
                        raise ValidationError(f"Invalid clips_by_style entry: {style}={count}")
                update_data["clips_by_style"] = clips_by_style
            
            if transaction:
                transaction.update(doc_ref, update_data)
            else:
                doc_ref.update(update_data)
            
            logger.debug(f"Updated video {video_id} status to {status}")
            return True
            
        except ValidationError:
            raise
        except Exception as e:
            logger.error(f"Failed to update video {video_id}: {e}", exc_info=True)
            raise VideoRepositoryError(f"Failed to update video: {e}") from e

    def get_video(self, video_id: str) -> Optional[VideoMetadata]:
        """
        Get video metadata.
        
        Args:
            video_id: Video ID
            
        Returns:
            VideoMetadata instance or None if not found
            
        Raises:
            VideoRepositoryError: If retrieval fails
        """
        try:
            doc = self.videos_collection.document(video_id).get()
            if doc.exists:
                data = doc.to_dict()
                if data:
                    return VideoMetadata.from_dict(data)
            return None
        except Exception as e:
            logger.error(f"Failed to get video {video_id}: {e}", exc_info=True)
            raise VideoRepositoryError(f"Failed to get video: {e}") from e

    def get_video_or_raise(self, video_id: str) -> VideoMetadata:
        """
        Get video metadata or raise NotFoundError.
        
        Args:
            video_id: Video ID
            
        Returns:
            VideoMetadata instance
            
        Raises:
            NotFoundError: If video not found
            VideoRepositoryError: If retrieval fails
        """
        video = self.get_video(video_id)
        if video is None:
            raise NotFoundError(f"Video {video_id} not found")
        return video

    def list_videos(
        self,
        status: Optional[str] = None,
        limit: Optional[int] = None,
        order_by: str = "created_at",
    ) -> List[VideoMetadata]:
        """
        List user's videos with optional filtering.
        
        Args:
            status: Optional status filter
            limit: Maximum number of results
            order_by: Field to order by ("created_at", "updated_at", "clips_count")
            
        Returns:
            List of VideoMetadata instances
            
        Raises:
            ValidationError: If parameters are invalid
            VideoRepositoryError: If query fails
        """
        try:
            if status and status not in ["processing", "completed", "failed"]:
                raise ValidationError(f"Invalid status: {status}")
            
            if limit is not None and (limit < 1 or limit > 1000):
                raise ValidationError(f"Invalid limit: {limit}")
            
            query = self.videos_collection
            
            if status:
                query = query.where("status", "==", status)
            
            # Order by
            if order_by == "created_at":
                query = query.order_by("created_at", direction=firestore.Query.DESCENDING)
            elif order_by == "updated_at":
                query = query.order_by("updated_at", direction=firestore.Query.DESCENDING)
            elif order_by == "clips_count":
                query = query.order_by("clips_count", direction=firestore.Query.DESCENDING)
            else:
                query = query.order_by("created_at", direction=firestore.Query.DESCENDING)
            
            if limit:
                query = query.limit(limit)
            
            videos = []
            for doc in query.stream():
                data = doc.to_dict()
                if data:
                    try:
                        videos.append(VideoMetadata.from_dict(data))
                    except Exception as e:
                        logger.warning(f"Failed to parse video {doc.id}: {e}")
                        continue
            
            return videos
            
        except ValidationError:
            raise
        except Exception as e:
            logger.error(f"Failed to list videos: {e}", exc_info=True)
            raise VideoRepositoryError(f"Failed to list videos: {e}") from e

    def update_clip_statistics(
        self,
        video_id: str,
        transaction: Optional[Transaction] = None,
    ) -> bool:
        """
        Update video clip statistics from clips subcollection.
        
        This aggregates statistics from clips and updates the video document.
        Should be called after clips are created/updated/deleted.
        
        Args:
            video_id: Video ID
            transaction: Optional Firestore transaction
            
        Returns:
            True if updated, False if video not found
            
        Raises:
            VideoRepositoryError: If update fails
        """
        try:
            clips_repo = ClipRepository(self.user_id, video_id)
            
            # Get all completed clips
            all_clips = clips_repo.list_clips(status="completed")
            total_count = len(all_clips)
            
            # Count by style (optimized)
            clips_by_style: Dict[str, int] = {}
            for clip in all_clips:
                style = clip.style
                clips_by_style[style] = clips_by_style.get(style, 0) + 1
            
            # Update video document
            return self.update_video_status(
                video_id,
                status="completed",  # Assume completed if updating stats
                clips_count=total_count,
                clips_by_style=clips_by_style,
                transaction=transaction,
            )
            
        except Exception as e:
            logger.error(
                f"Failed to update clip statistics for video {video_id}: {e}",
                exc_info=True
            )
            raise VideoRepositoryError(f"Failed to update clip statistics: {e}") from e

    def delete_video(
        self,
        video_id: str,
        delete_clips: bool = True,
        transaction: Optional[Transaction] = None,
    ) -> bool:
        """
        Delete video metadata and optionally clips subcollection.
        
        Args:
            video_id: Video ID
            delete_clips: Whether to delete clips subcollection
            transaction: Optional Firestore transaction
            
        Returns:
            True if deleted, False if not found
            
        Raises:
            VideoRepositoryError: If deletion fails
        """
        try:
            doc_ref = self.videos_collection.document(video_id)
            
            if transaction:
                doc = doc_ref.get(transaction=transaction)
                if not doc.exists:
                    return False
                
                # Delete clips if requested
                if delete_clips:
                    clips_repo = ClipRepository(self.user_id, video_id)
                    clips_repo.delete_all_clips(transaction=transaction)
                
                transaction.delete(doc_ref)
            else:
                doc = doc_ref.get()
                if not doc.exists:
                    return False
                
                # Delete clips if requested
                if delete_clips:
                    clips_repo = ClipRepository(self.user_id, video_id)
                    clips_repo.delete_all_clips()
                
                doc_ref.delete()
            
            logger.debug(f"Deleted video metadata: {video_id}")
            return True
            
        except Exception as e:
            logger.error(f"Failed to delete video {video_id}: {e}", exc_info=True)
            raise VideoRepositoryError(f"Failed to delete video: {e}") from e

    def count_videos(self, status: Optional[str] = None) -> int:
        """
        Count videos matching filter.
        
        Args:
            status: Optional status filter
            
        Returns:
            Number of matching videos
            
        Raises:
            VideoRepositoryError: If count fails
        """
        try:
            query = self.videos_collection
            
            if status:
                query = query.where("status", "==", status)
            
            # Use len() for now, can be optimized with count queries
            return len(list(query.stream()))
            
        except Exception as e:
            logger.error(f"Failed to count videos: {e}", exc_info=True)
            raise VideoRepositoryError(f"Failed to count videos: {e}") from e

