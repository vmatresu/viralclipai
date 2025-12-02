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
    
    # Resolve credentials path - check multiple possible locations
    if not os.path.isabs(credentials_path):
        # Get the app root directory (/app in container)
        app_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        app_root = os.path.dirname(app_dir)  # /app (parent of /app/app)
        
        # Check common locations (order matters - check most likely first)
        possible_paths = [
            os.path.join(app_root, credentials_path),  # /app/firebase-credentials.json (dev volume mount)
            os.path.join(app_root, os.path.basename(credentials_path)),  # /app/firebase-credentials.json (if path includes dir)
            os.path.join(app_dir, credentials_path),  # /app/app/firebase-credentials.json (Docker build)
            credentials_path,  # Try as-is (current working directory)
        ]
        
        for path in possible_paths:
            if os.path.exists(path):
                credentials_path = os.path.abspath(path)
                logger.debug("Found Firebase credentials at: %s", credentials_path)
                break
        else:
            # If none found, provide helpful error message
            raise FileNotFoundError(
                f"Firebase credentials file not found. Tried: {', '.join(possible_paths)}. "
                f"Set FIREBASE_CREDENTIALS_PATH to an absolute path or ensure the file exists."
            )
    
    if not os.path.exists(credentials_path):
        raise FileNotFoundError(f"Firebase credentials file not found at: {credentials_path}")
    
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
        # Allow 5 minutes of clock skew for dev environments (docker vs host time drift)
        return auth.verify_id_token(id_token, clock_skew_seconds=300)
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
