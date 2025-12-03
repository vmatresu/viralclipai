"""
Clip metadata repository for Firestore.

This module provides a clean interface for managing clip metadata in Firestore,
following repository pattern and separation of concerns.
"""

import logging
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from google.cloud import firestore

from app.core.firebase_client import get_firestore_client

logger = logging.getLogger(__name__)


class ClipRepository:
    """
    Repository for managing clip metadata in Firestore.
    
    Follows Repository Pattern for clean separation of data access logic.
    """

    def __init__(self, user_id: str, video_id: str):
        """
        Initialize clip repository.
        
        Args:
            user_id: User ID
            video_id: Video ID
        """
        self.user_id = user_id
        self.video_id = video_id
        self.db = get_firestore_client()
        self.clips_collection = (
            self.db.collection("users")
            .document(user_id)
            .collection("videos")
            .document(video_id)
            .collection("clips")
        )

    def create_clip(
        self,
        clip_id: str,
        filename: str,
        scene_id: int,
        scene_title: str,
        style: str,
        start_time: str,
        end_time: str,
        duration_seconds: float,
        priority: int = 99,
        scene_description: Optional[str] = None,
        file_size_bytes: int = 0,
        has_thumbnail: bool = False,
    ) -> Dict[str, Any]:
        """
        Create a clip metadata document in Firestore.
        
        Args:
            clip_id: Unique clip ID (filename without extension)
            filename: R2 filename
            scene_id: Scene/highlight ID
            scene_title: Scene title
            style: Clip style
            start_time: Start time in HH:MM:SS format
            end_time: End time in HH:MM:SS format
            duration_seconds: Duration in seconds
            priority: Processing priority
            scene_description: Optional scene description
            file_size_bytes: File size in bytes
            has_thumbnail: Whether thumbnail exists
            
        Returns:
            Created clip document data
        """
        now = datetime.now(timezone.utc)
        
        clip_data: Dict[str, Any] = {
            "clip_id": clip_id,
            "video_id": self.video_id,
            "user_id": self.user_id,
            "scene_id": scene_id,
            "scene_title": scene_title,
            "filename": filename,
            "style": style,
            "priority": priority,
            "start_time": start_time,
            "end_time": end_time,
            "duration_seconds": duration_seconds,
            "file_size_bytes": file_size_bytes,
            "file_size_mb": round(file_size_bytes / (1024 * 1024), 2) if file_size_bytes > 0 else 0.0,
            "has_thumbnail": has_thumbnail,
            "r2_key": f"{self.user_id}/{self.video_id}/clips/{filename}",
            "thumbnail_r2_key": (
                f"{self.user_id}/{self.video_id}/clips/{filename.rsplit('.', 1)[0]}.jpg"
                if has_thumbnail
                else None
            ),
            "status": "processing",
            "created_at": now,
            "created_by": self.user_id,
        }
        
        if scene_description:
            clip_data["scene_description"] = scene_description

        # Create document with clip_id as document ID
        self.clips_collection.document(clip_id).set(clip_data)
        
        logger.debug(f"Created clip metadata: {clip_id} for video {self.video_id}")
        return clip_data

    def update_clip_status(
        self,
        clip_id: str,
        status: str,
        file_size_bytes: Optional[int] = None,
        has_thumbnail: Optional[bool] = None,
    ) -> bool:
        """
        Update clip status and optional file information.
        
        Args:
            clip_id: Clip ID
            status: New status ("processing", "completed", "failed")
            file_size_bytes: Optional file size in bytes
            has_thumbnail: Optional thumbnail flag
            
        Returns:
            True if updated, False if clip not found
        """
        doc_ref = self.clips_collection.document(clip_id)
        doc = doc_ref.get()
        
        if not doc.exists:
            logger.warning(f"Clip {clip_id} not found for update")
            return False
        
        update_data: Dict[str, Any] = {
            "status": status,
            "updated_at": datetime.now(timezone.utc),
        }
        
        if status == "completed":
            update_data["completed_at"] = datetime.now(timezone.utc)
        
        if file_size_bytes is not None:
            update_data["file_size_bytes"] = file_size_bytes
            update_data["file_size_mb"] = round(file_size_bytes / (1024 * 1024), 2)
        
        if has_thumbnail is not None:
            update_data["has_thumbnail"] = has_thumbnail
            if has_thumbnail:
                filename = doc.get("filename", "")
                if filename:
                    update_data["thumbnail_r2_key"] = (
                        f"{self.user_id}/{self.video_id}/clips/{filename.rsplit('.', 1)[0]}.jpg"
                    )
        
        doc_ref.update(update_data)
        logger.debug(f"Updated clip {clip_id} status to {status}")
        return True

    def list_clips(
        self,
        style: Optional[str] = None,
        scene_id: Optional[int] = None,
        status: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        """
        List clips with optional filtering.
        
        Args:
            style: Filter by style
            scene_id: Filter by scene ID
            status: Filter by status
            limit: Maximum number of results
            
        Returns:
            List of clip documents
        """
        query = self.clips_collection
        
        # Apply filters
        if style:
            query = query.where("style", "==", style)
        if scene_id is not None:
            query = query.where("scene_id", "==", scene_id)
        if status:
            query = query.where("status", "==", status)
        
        # Order by priority (or created_at as fallback)
        query = query.order_by("priority").order_by("created_at")
        
        if limit:
            query = query.limit(limit)
        
        clips = []
        for doc in query.stream():
            clip_data = doc.to_dict()
            if clip_data:
                clips.append(clip_data)
        
        return clips

    def get_clip(self, clip_id: str) -> Optional[Dict[str, Any]]:
        """
        Get a single clip by ID.
        
        Args:
            clip_id: Clip ID
            
        Returns:
            Clip document or None if not found
        """
        doc = self.clips_collection.document(clip_id).get()
        if doc.exists:
            return doc.to_dict()
        return None

    def count_clips(
        self,
        style: Optional[str] = None,
        scene_id: Optional[int] = None,
        status: Optional[str] = None,
    ) -> int:
        """
        Count clips matching filters.
        
        Args:
            style: Filter by style
            scene_id: Filter by scene ID
            status: Filter by status
            
        Returns:
            Number of matching clips
        """
        query = self.clips_collection
        
        if style:
            query = query.where("style", "==", style)
        if scene_id is not None:
            query = query.where("scene_id", "==", scene_id)
        if status:
            query = query.where("status", "==", status)
        
        # Use count() for efficient counting
        return len(list(query.stream()))

    def delete_clip(self, clip_id: str) -> bool:
        """
        Delete a clip document.
        
        Args:
            clip_id: Clip ID
            
        Returns:
            True if deleted, False if not found
        """
        doc_ref = self.clips_collection.document(clip_id)
        doc = doc_ref.get()
        
        if not doc.exists:
            return False
        
        doc_ref.delete()
        logger.debug(f"Deleted clip metadata: {clip_id}")
        return True

    def delete_all_clips(self) -> int:
        """
        Delete all clips for this video.
        
        Returns:
            Number of clips deleted
        """
        deleted_count = 0
        batch = self.db.batch()
        batch_size = 0
        max_batch_size = 500  # Firestore batch limit
        
        for doc in self.clips_collection.stream():
            batch.delete(doc.reference)
            batch_size += 1
            deleted_count += 1
            
            if batch_size >= max_batch_size:
                batch.commit()
                batch = self.db.batch()
                batch_size = 0
        
        if batch_size > 0:
            batch.commit()
        
        logger.info(f"Deleted {deleted_count} clip metadata documents for video {self.video_id}")
        return deleted_count

