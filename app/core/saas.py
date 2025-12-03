from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

from google.cloud import firestore
from google.cloud.firestore_v1 import FieldFilter

from app.core.firebase_client import get_firestore_client
from app.core.plans.service import PlanService

# Legacy constant for backward compatibility
# Plans are now managed via PlanService and stored in Firestore
PLANS: Dict[str, Dict[str, int]] = {
    "free": {"max_clips_per_month": 20},
    "pro": {"max_clips_per_month": 500},
}

SUPERADMIN_ROLE = "superadmin"

# Global plan service instance
_plan_service: Optional[PlanService] = None


def get_plan_service() -> PlanService:
    """Get the global plan service instance."""
    global _plan_service
    if _plan_service is None:
        _plan_service = PlanService()
    return _plan_service


def get_or_create_user(uid: str, email: Optional[str] = None) -> Dict[str, Any]:
    db = get_firestore_client()
    ref = db.collection("users").document(uid)
    snap = ref.get()
    if not snap.exists:
        data: Dict[str, Any] = {
            "uid": uid,
            "email": email,
            "plan": "free",
            "created_at": datetime.now(timezone.utc),
        }
        ref.set(data)
        return data
    data = snap.to_dict() or {}
    if email and not data.get("email"):
        ref.update({"email": email})
        data["email"] = email
    return data


def get_plan_limits_for_user(uid: str) -> Tuple[str, int]:
    """
    Get plan limits for a user.
    
    Uses the new PlanService to fetch limits from Firestore with fallback
    to legacy hardcoded plans for backward compatibility.
    """
    user = get_or_create_user(uid)
    plan_id = user.get("plan", "free")
    
    # Try to get limits from Firestore via PlanService
    try:
        plan_service = get_plan_service()
        max_clips = plan_service.get_plan_limit_value(
            plan_id,
            "max_clips_per_month",
            default=PLANS.get("free", {}).get("max_clips_per_month", 20)
        )
        return plan_id, max_clips
    except Exception:
        # Fallback to legacy hardcoded plans
        plan = PLANS.get(plan_id, PLANS["free"])
        return plan_id, int(plan.get("max_clips_per_month", PLANS["free"]["max_clips_per_month"]))


def get_monthly_usage(uid: str, as_of: Optional[datetime] = None) -> int:
    if as_of is None:
        as_of = datetime.now(timezone.utc)
    month_start = datetime(as_of.year, as_of.month, 1, tzinfo=timezone.utc)
    db = get_firestore_client()
    col = db.collection("users").document(uid).collection("videos")
    query = col.where(filter=FieldFilter("created_at", ">=", month_start))
    total = 0
    for doc in query.stream():
        payload = doc.to_dict() or {}
        total += int(payload.get("clips_count", 0))
    return total


def record_video_job(
    uid: str,
    run_id: str,
    video_url: str,
    video_title: str,
    clips_count: int,
    custom_prompt: Optional[str] = None,
    status: str = "completed",
) -> None:
    """
    Record or update a video job in Firestore.
    
    Args:
        uid: User ID
        run_id: Video run ID
        video_url: Source video URL
        video_title: Video title
        clips_count: Number of clips
        custom_prompt: Optional custom prompt
        status: Processing status ("processing" or "completed")
    """
    db = get_firestore_client()
    col = db.collection("users").document(uid).collection("videos")
    now = datetime.now(timezone.utc)
    payload: Dict[str, Any] = {
        "video_id": run_id,
        "video_url": video_url,
        "video_title": video_title,
        "clips_count": int(clips_count),
        "status": status,
        "created_at": now,
    }
    if custom_prompt:
        payload["custom_prompt"] = custom_prompt
    if status == "completed":
        payload["completed_at"] = now

    col.document(run_id).set(payload, merge=True)


def update_video_status(
    uid: str,
    run_id: str,
    status: str,
    clips_count: Optional[int] = None,
) -> bool:
    """
    Update video processing status.
    
    Args:
        uid: User ID
        run_id: Video run ID
        status: New status ("processing" or "completed")
        clips_count: Optional updated clips count
        
    Returns:
        True if video was updated, False if it didn't exist
    """
    db = get_firestore_client()
    doc_ref = db.collection("users").document(uid).collection("videos").document(run_id)
    doc = doc_ref.get()
    
    if not doc.exists:
        return False
    
    update_data: Dict[str, Any] = {
        "status": status,
    }
    if status == "completed":
        update_data["completed_at"] = datetime.now(timezone.utc)
    if clips_count is not None:
        update_data["clips_count"] = int(clips_count)
    
    doc_ref.update(update_data)
    return True


def user_owns_video(uid: str, run_id: str) -> bool:
    db = get_firestore_client()
    doc = db.collection("users").document(uid).collection("videos").document(run_id).get()
    return doc.exists


def get_video_metadata(uid: str, video_id: str) -> Optional[Dict[str, Any]]:
    """Get video metadata (title, URL) from Firestore."""
    db = get_firestore_client()
    doc = db.collection("users").document(uid).collection("videos").document(video_id).get()
    if not doc.exists:
        return None
    data = doc.to_dict() or {}
    return {
        "video_title": data.get("video_title"),
        "video_url": data.get("video_url"),
    }


def update_video_title(uid: str, video_id: str, new_title: str) -> bool:
    """Update video title in Firestore.
    
    Args:
        uid: User ID
        video_id: Video ID to update
        new_title: New title to set
        
    Returns:
        True if video was updated, False if it didn't exist
        
    Raises:
        Exception: If update fails
    """
    db = get_firestore_client()
    doc_ref = db.collection("users").document(uid).collection("videos").document(video_id)
    doc = doc_ref.get()
    
    if not doc.exists:
        return False
    
    doc_ref.update({"video_title": new_title})
    return True


def list_user_videos(uid: str) -> List[Dict[str, Any]]:
    db = get_firestore_client()
    col = db.collection("users").document(uid).collection("videos")
    docs = col.order_by("created_at", direction=firestore.Query.DESCENDING).stream()
    results: List[Dict[str, Any]] = []
    for doc in docs:
        payload = doc.to_dict() or {}
        payload["id"] = doc.id
        results.append(payload)
    return results


def is_super_admin(uid: str) -> bool:
    """Return True if the user has the superadmin role.

    Role is stored on the user document as `role = "superadmin"` and can be
    managed via the Firestore console.
    """
    db = get_firestore_client()
    doc = db.collection("users").document(uid).get()
    if not doc.exists:
        return False
    data = doc.to_dict() or {}
    return data.get("role") == SUPERADMIN_ROLE


def get_user_settings(uid: str) -> Dict[str, Any]:
    db = get_firestore_client()
    ref = db.collection("users").document(uid)
    snap = ref.get()
    if not snap.exists:
        data = get_or_create_user(uid)
    else:
        data = snap.to_dict() or {}
    settings = data.get("settings") or {}
    return settings


def update_user_settings(uid: str, settings: Dict[str, Any]) -> Dict[str, Any]:
    db = get_firestore_client()
    ref = db.collection("users").document(uid)
    ref.set({"settings": settings}, merge=True)
    return settings


def get_global_prompt() -> Optional[str]:
    """Fetch the global base prompt from Firestore admin config.

    Stored in `admin/config` document under the `base_prompt` field. Returns
    None if not set.
    """
    db = get_firestore_client()
    doc = db.collection("admin").document("config").get()
    if not doc.exists:
        return None
    data = doc.to_dict() or {}
    prompt = data.get("base_prompt")
    if isinstance(prompt, str) and prompt.strip():
        return prompt.strip()
    return None


def set_global_prompt(uid: str, prompt: str) -> str:
    """Update the global base prompt in Firestore admin config.

    Records the updating user and timestamp for traceability.
    """
    db = get_firestore_client()
    ref = db.collection("admin").document("config")
    now = datetime.now(timezone.utc)
    payload: Dict[str, Any] = {
        "base_prompt": prompt,
        "updated_at": now,
        "updated_by": uid,
    }
    ref.set(payload, merge=True)
    return prompt


def delete_video(uid: str, video_id: str) -> bool:
    """
    Delete a single video record from Firestore.
    
    Args:
        uid: User ID
        video_id: Video ID to delete
        
    Returns:
        True if video was deleted, False if it didn't exist
        
    Raises:
        Exception: If deletion fails
    """
    db = get_firestore_client()
    doc_ref = db.collection("users").document(uid).collection("videos").document(video_id)
    doc = doc_ref.get()
    
    if not doc.exists:
        return False
    
    doc_ref.delete()
    return True


def delete_videos(uid: str, video_ids: List[str]) -> Dict[str, bool]:
    """
    Delete multiple video records from Firestore.
    
    Args:
        uid: User ID
        video_ids: List of video IDs to delete
        
    Returns:
        Dictionary mapping video_id to deletion success status
        
    Raises:
        Exception: If deletion fails
    """
    if not video_ids:
        return {}
    
    db = get_firestore_client()
    col_ref = db.collection("users").document(uid).collection("videos")
    
    results: Dict[str, bool] = {}
    
    # Use batch writes for efficiency (Firestore allows up to 500 operations per batch)
    batch_size = 500
    for i in range(0, len(video_ids), batch_size):
        batch = db.batch()
        batch_video_ids = video_ids[i:i + batch_size]
        
        for video_id in batch_video_ids:
            doc_ref = col_ref.document(video_id)
            doc = doc_ref.get()
            if doc.exists:
                batch.delete(doc_ref)
                results[video_id] = True
            else:
                results[video_id] = False
        
        if batch_video_ids:
            batch.commit()
    
    return results
