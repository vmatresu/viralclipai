import os
from typing import Any, Dict, Optional

import firebase_admin
from firebase_admin import auth, credentials, firestore
from fastapi import Header, HTTPException, status

from app.config import logger

_firebase_app: Optional[firebase_admin.App] = None
_db: Optional[firestore.Client] = None


def _init_firebase() -> None:
    global _firebase_app, _db
    if _firebase_app is not None and _db is not None:
        return
    project_id = os.getenv("FIREBASE_PROJECT_ID")
    credentials_path = os.getenv("FIREBASE_CREDENTIALS_PATH")
    if not project_id or not credentials_path:
        raise RuntimeError("FIREBASE_PROJECT_ID and FIREBASE_CREDENTIALS_PATH must be configured")
    cred = credentials.Certificate(credentials_path)
    _firebase_app = firebase_admin.initialize_app(cred, {"projectId": project_id})
    _db = firestore.client()
    logger.info("Firebase initialized for project %s", project_id)


def get_firestore_client() -> firestore.Client:
    if _db is None:
        _init_firebase()
    assert _db is not None
    return _db


def verify_id_token(id_token: str) -> Dict[str, Any]:
    _init_firebase()
    try:
        return auth.verify_id_token(id_token)
    except Exception as exc:
        logger.warning("Failed to verify Firebase ID token: %s", exc)
        raise


async def get_current_user(authorization: Optional[str] = Header(None)) -> Dict[str, Any]:
    if not authorization or not authorization.startswith("Bearer "):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Missing Authorization header")
    token = authorization.split(" ", 1)[1]
    try:
        decoded = verify_id_token(token)
    except Exception:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid or expired token")
    uid = decoded.get("uid")
    if not uid:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token payload")
    return {"uid": uid, "email": decoded.get("email")}
