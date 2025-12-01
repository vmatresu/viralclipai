from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

from google.cloud import firestore

from app.core.firebase_client import get_firestore_client

PLANS: Dict[str, Dict[str, int]] = {
    "free": {"max_clips_per_month": 20},
    "pro": {"max_clips_per_month": 500},
}


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
    user = get_or_create_user(uid)
    plan_id = user.get("plan", "free")
    plan = PLANS.get(plan_id, PLANS["free"])
    return plan_id, int(plan.get("max_clips_per_month", PLANS["free"]["max_clips_per_month"]))


def get_monthly_usage(uid: str, as_of: Optional[datetime] = None) -> int:
    if as_of is None:
        as_of = datetime.now(timezone.utc)
    month_start = datetime(as_of.year, as_of.month, 1, tzinfo=timezone.utc)
    db = get_firestore_client()
    col = db.collection("users").document(uid).collection("videos")
    query = col.where("created_at", ">=", month_start)
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
) -> None:
    db = get_firestore_client()
    col = db.collection("users").document(uid).collection("videos")
    now = datetime.now(timezone.utc)
    payload: Dict[str, Any] = {
        "video_id": run_id,
        "video_url": video_url,
        "video_title": video_title,
        "clips_count": int(clips_count),
        "created_at": now,
    }
    if custom_prompt:
        payload["custom_prompt"] = custom_prompt

    col.document(run_id).set(payload, merge=True)


def user_owns_video(uid: str, run_id: str) -> bool:
    db = get_firestore_client()
    doc = db.collection("users").document(uid).collection("videos").document(run_id).get()
    return doc.exists


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
