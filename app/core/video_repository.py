"""
Video metadata repository for Firestore.

This module provides a clean interface for managing video metadata in Firestore,
including clip statistics and highlights summary.
"""

import logging
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from google.cloud import firestore

from app.core.firebase_client import get_firestore_client

logger = logging.getLogger(__name__)


class VideoRepository:
    """
    Repository for managing video metadata in Firestore.
    
    Follows Repository Pattern for clean separation of data access logic.
    """

    def __init__(self, user_id: str):
        """
        Initialize video repository.
        
        Args:
            user_id: User ID
        """
        self.user_id = user_id
        self.db = get_firestore_client()
        self.videos_collection = (
            self.db.collection("users")
            .document(user_id)
            .collection("videos")
        )

    def create_or_update_video(
        self,
        video_id: str,
        video_url: str,
        video_title: str,
        youtube_id: str,
        status: str = "processing",
        custom_prompt: Optional[str] = None,
        styles_processed: Optional[List[str]] = None,
        crop_mode: str = "none",
        target_aspect: str = "9:16",
        highlights_count: int = 0,
        highlights_summary: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """
        Create or update video metadata.
        
        Args:
            video_id: Video ID (run_id)
            video_url: Source video URL
            video_title: Video title
            youtube_id: YouTube ID
            status: Processing status
            custom_prompt: Optional custom prompt
            styles_processed: List of styles processed
            crop_mode: Crop mode used
            target_aspect: Target aspect ratio
            highlights_count: Number of highlights detected
            highlights_summary: Optional highlights summary
            
        Returns:
            Video document data
        """
        now = datetime.now(timezone.utc)
        
        video_data: Dict[str, Any] = {
            "video_id": video_id,
            "user_id": self.user_id,
            "video_url": video_url,
            "video_title": video_title,
            "youtube_id": youtube_id,
            "status": status,
            "created_at": now,
            "updated_at": now,
            "clips_count": 0,
            "clips_by_style": {},
            "highlights_count": highlights_count,
            "highlights_json_key": f"{self.user_id}/{video_id}/highlights.json",
            "styles_processed": styles_processed or [],
            "crop_mode": crop_mode,
            "target_aspect": target_aspect,
            "created_by": self.user_id,
        }
        
        if custom_prompt:
            video_data["custom_prompt"] = custom_prompt
        
        if highlights_summary:
            video_data["highlights_summary"] = highlights_summary
        
        if status == "completed":
            video_data["completed_at"] = now
        
        # Use set with merge=True to support updates
        self.videos_collection.document(video_id).set(video_data, merge=True)
        
        logger.debug(f"Created/updated video metadata: {video_id}")
        return video_data

    def update_video_status(
        self,
        video_id: str,
        status: str,
        clips_count: Optional[int] = None,
        clips_by_style: Optional[Dict[str, int]] = None,
    ) -> bool:
        """
        Update video status and statistics.
        
        Args:
            video_id: Video ID
            status: New status
            clips_count: Optional total clips count
            clips_by_style: Optional clips count by style
            
        Returns:
            True if updated, False if not found
        """
        doc_ref = self.videos_collection.document(video_id)
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
        
        if clips_count is not None:
            update_data["clips_count"] = clips_count
        
        if clips_by_style is not None:
            update_data["clips_by_style"] = clips_by_style
        
        doc_ref.update(update_data)
        logger.debug(f"Updated video {video_id} status to {status}")
        return True

    def get_video(self, video_id: str) -> Optional[Dict[str, Any]]:
        """
        Get video metadata.
        
        Args:
            video_id: Video ID
            
        Returns:
            Video document or None if not found
        """
        doc = self.videos_collection.document(video_id).get()
        if doc.exists:
            return doc.to_dict()
        return None

    def list_videos(
        self,
        status: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        """
        List user's videos.
        
        Args:
            status: Optional status filter
            limit: Maximum number of results
            
        Returns:
            List of video documents
        """
        query = self.videos_collection.order_by("created_at", direction=firestore.Query.DESCENDING)
        
        if status:
            query = query.where("status", "==", status)
        
        if limit:
            query = query.limit(limit)
        
        videos = []
        for doc in query.stream():
            video_data = doc.to_dict()
            if video_data:
                videos.append(video_data)
        
        return videos

    def update_clip_statistics(self, video_id: str) -> bool:
        """
        Update video clip statistics from clips subcollection.
        
        This should be called after clips are created/updated/deleted
        to keep video statistics in sync.
        
        Args:
            video_id: Video ID
            
        Returns:
            True if updated, False if video not found
        """
        from app.core.clips_repository import ClipRepository
        
        clips_repo = ClipRepository(self.user_id, video_id)
        
        # Count all completed clips
        all_clips = clips_repo.list_clips(status="completed")
        total_count = len(all_clips)
        
        # Count by style
        clips_by_style: Dict[str, int] = {}
        for clip in all_clips:
            style = clip.get("style", "unknown")
            clips_by_style[style] = clips_by_style.get(style, 0) + 1
        
        # Update video document
        return self.update_video_status(
            video_id,
            status="completed",  # Assume completed if updating stats
            clips_count=total_count,
            clips_by_style=clips_by_style,
        )

    def delete_video(self, video_id: str) -> bool:
        """
        Delete video metadata (clips subcollection should be deleted separately).
        
        Args:
            video_id: Video ID
            
        Returns:
            True if deleted, False if not found
        """
        doc_ref = self.videos_collection.document(video_id)
        doc = doc_ref.get()
        
        if not doc.exists:
            return False
        
        doc_ref.delete()
        logger.debug(f"Deleted video metadata: {video_id}")
        return True

