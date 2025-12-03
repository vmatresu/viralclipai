"""
Plan Repository

Data access layer for plan management in Firestore.
Implements caching for performance optimization.
"""

import logging
from datetime import datetime, timezone
from typing import Dict, List, Optional
from functools import lru_cache
from threading import Lock

from google.cloud import firestore
from google.cloud.firestore_v1 import FieldFilter

from app.core.firebase_client import get_firestore_client
from app.core.plans.models import Plan, PlanStatus
from app.config import logger


class PlanRepository:
    """
    Repository for plan data access with in-memory caching.
    
    Thread-safe caching layer that refreshes on updates.
    For production with multiple workers, consider Redis-based caching.
    """
    
    _cache: Dict[str, Plan] = {}
    _cache_lock = Lock()
    _cache_ttl_seconds = 300  # 5 minutes cache TTL
    _last_cache_update: Optional[float] = None
    
    COLLECTION_NAME = "plans"
    
    @classmethod
    def _get_collection(cls) -> firestore.CollectionReference:
        """Get Firestore plans collection."""
        db = get_firestore_client()
        return db.collection(cls.COLLECTION_NAME)
    
    @classmethod
    def _is_cache_valid(cls) -> bool:
        """Check if cache is still valid."""
        if cls._last_cache_update is None:
            return False
        
        import time
        age = time.time() - cls._last_cache_update
        return age < cls._cache_ttl_seconds
    
    @classmethod
    def _invalidate_cache(cls) -> None:
        """Invalidate the cache."""
        with cls._cache_lock:
            cls._cache.clear()
            cls._last_cache_update = None
            logger.debug("Plan cache invalidated")
    
    @classmethod
    def _refresh_cache(cls) -> None:
        """Refresh cache from Firestore."""
        with cls._cache_lock:
            if cls._is_cache_valid():
                return  # Cache still valid
            
            try:
                collection = cls._get_collection()
                docs = collection.where(filter=FieldFilter("status", "==", PlanStatus.ACTIVE.value)).stream()
                
                new_cache: Dict[str, Plan] = {}
                for doc in docs:
                    data = doc.to_dict() or {}
                    data["id"] = doc.id  # Ensure ID is set
                    try:
                        plan = Plan.from_firestore_dict(data)
                        new_cache[plan.id] = plan
                    except Exception as e:
                        logger.error("Failed to parse plan %s: %s", doc.id, e)
                        continue
                
                cls._cache = new_cache
                import time
                cls._last_cache_update = time.time()
                logger.debug("Plan cache refreshed: %d plans loaded", len(new_cache))
            except Exception as e:
                logger.error("Failed to refresh plan cache: %s", e)
                # Keep existing cache if refresh fails
    
    @classmethod
    def get_by_id(cls, plan_id: str, use_cache: bool = True) -> Optional[Plan]:
        """
        Get a plan by ID.
        
        Args:
            plan_id: Plan identifier
            use_cache: Whether to use cache (default: True)
            
        Returns:
            Plan if found, None otherwise
        """
        plan_id = plan_id.lower().strip()
        
        if use_cache:
            cls._refresh_cache()
            with cls._cache_lock:
                if plan_id in cls._cache:
                    return cls._cache[plan_id]
        
        # Fallback to direct Firestore query
        try:
            doc = cls._get_collection().document(plan_id).get()
            if not doc.exists:
                return None
            
            data = doc.to_dict() or {}
            data["id"] = doc.id
            plan = Plan.from_firestore_dict(data)
            
            # Update cache
            if use_cache:
                with cls._cache_lock:
                    cls._cache[plan_id] = plan
            
            return plan
        except Exception as e:
            logger.error("Failed to fetch plan %s from Firestore: %s", plan_id, e)
            return None
    
    @classmethod
    def get_all(cls, include_inactive: bool = False) -> List[Plan]:
        """
        Get all plans.
        
        Args:
            include_inactive: Whether to include inactive/archived plans
            
        Returns:
            List of plans
        """
        cls._refresh_cache()
        
        with cls._cache_lock:
            plans = list(cls._cache.values())
        
        if not include_inactive:
            plans = [p for p in plans if p.status == PlanStatus.ACTIVE]
        
        # Sort by ID for consistent ordering
        return sorted(plans, key=lambda p: p.id)
    
    @classmethod
    def create(cls, plan: Plan, created_by: str) -> Plan:
        """
        Create a new plan in Firestore.
        
        Args:
            plan: Plan to create
            created_by: User ID creating the plan
            
        Returns:
            Created plan
            
        Raises:
            ValueError: If plan already exists
        """
        plan_id = plan.id.lower().strip()
        
        # Check if plan exists
        existing = cls.get_by_id(plan_id, use_cache=False)
        if existing:
            raise ValueError(f"Plan '{plan_id}' already exists")
        
        # Set metadata
        now = datetime.now(timezone.utc)
        plan.created_at = now
        plan.updated_at = now
        plan.created_by = created_by
        plan.updated_by = created_by
        
        try:
            collection = cls._get_collection()
            collection.document(plan_id).set(plan.to_firestore_dict())
            
            # Invalidate cache
            cls._invalidate_cache()
            
            logger.info("Created plan: %s (by user %s)", plan_id, created_by)
            return plan
        except Exception as e:
            logger.error("Failed to create plan %s: %s", plan_id, e)
            raise
    
    @classmethod
    def update(cls, plan_id: str, updates: Dict, updated_by: str) -> Optional[Plan]:
        """
        Update an existing plan.
        
        Args:
            plan_id: Plan identifier
            updates: Dictionary of fields to update
            updated_by: User ID updating the plan
            
        Returns:
            Updated plan if found, None otherwise
        """
        plan_id = plan_id.lower().strip()
        
        doc_ref = cls._get_collection().document(plan_id)
        doc = doc_ref.get()
        
        if not doc.exists:
            return None
        
        # Prepare update data
        update_data = dict(updates)
        update_data["updated_at"] = datetime.now(timezone.utc)
        update_data["updated_by"] = updated_by
        
        try:
            doc_ref.update(update_data)
            
            # Invalidate cache
            cls._invalidate_cache()
            
            logger.info("Updated plan: %s (by user %s)", plan_id, updated_by)
            
            # Return updated plan
            return cls.get_by_id(plan_id, use_cache=False)
        except Exception as e:
            logger.error("Failed to update plan %s: %s", plan_id, e)
            raise
    
    @classmethod
    def delete(cls, plan_id: str) -> bool:
        """
        Delete a plan (soft delete by setting status to archived).
        
        Args:
            plan_id: Plan identifier
            
        Returns:
            True if deleted, False if not found
        """
        plan_id = plan_id.lower().strip()
        
        plan = cls.get_by_id(plan_id, use_cache=False)
        if not plan:
            return False
        
        # Soft delete by archiving
        cls.update(
            plan_id,
            {"status": PlanStatus.ARCHIVED.value},
            updated_by="system"
        )
        
        logger.info("Archived plan: %s", plan_id)
        return True
    
    @classmethod
    def get_default_plan_id(cls) -> str:
        """Get the default plan ID (typically 'free')."""
        return "free"

