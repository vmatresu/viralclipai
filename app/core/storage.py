import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import boto3
from botocore.client import Config
from botocore.exceptions import ClientError

from app.config import (
    R2_ACCESS_KEY_ID,
    R2_BUCKET_NAME,
    R2_ENDPOINT_URL,
    R2_SECRET_ACCESS_KEY,
    logger,
)

_r2_client: Optional[Any] = None


def get_r2_client():
    global _r2_client
    if _r2_client is None:
        session = boto3.session.Session()
        _r2_client = session.client(
            "s3",
            endpoint_url=R2_ENDPOINT_URL or None,
            aws_access_key_id=R2_ACCESS_KEY_ID or None,
            aws_secret_access_key=R2_SECRET_ACCESS_KEY or None,
            # Cloudflare R2 requires signature version 4 (sigv4)
            config=Config(signature_version='s3v4'),
            region_name="auto",  # R2 requires a region, 'auto' or 'us-east-1' usually works
        )
    return _r2_client


def upload_file(path: Path, key: str, content_type: str) -> None:
    client = get_r2_client()
    extra_args: Dict[str, Any] = {"ContentType": content_type}
    client.upload_file(str(path), R2_BUCKET_NAME, key, ExtraArgs=extra_args)


def generate_presigned_url(key: str, expires_in: int = 3600) -> str:
    client = get_r2_client()
    try:
        url = client.generate_presigned_url(
            "get_object",
            Params={"Bucket": R2_BUCKET_NAME, "Key": key},
            ExpiresIn=expires_in,
        )
        return url
    except ClientError as exc:
        logger.error("Failed to generate presigned URL for %s: %s", key, exc)
        return ""


def load_highlights(uid: str, video_id: str) -> Dict[str, Any]:
    client = get_r2_client()
    key = f"{uid}/{video_id}/highlights.json"
    try:
        obj = client.get_object(Bucket=R2_BUCKET_NAME, Key=key)
    except ClientError as exc:
        logger.error("Failed to load highlights for %s/%s: %s", uid, video_id, exc)
        return {}
    body = obj["Body"].read()
    try:
        return json.loads(body)
    except Exception:
        return {}


def get_object(key: str, Range: Optional[str] = None) -> Optional[Any]:
    """Get an object from R2 storage, optionally with a byte range."""
    client = get_r2_client()
    try:
        params = {"Bucket": R2_BUCKET_NAME, "Key": key}
        if Range:
            params["Range"] = Range
        obj = client.get_object(**params)
        return obj
    except ClientError as exc:
        logger.error("Failed to get object %s: %s", key, exc)
        return None


def list_clips_with_metadata(uid: str, video_id: str, highlights_map: Dict[int, Dict[str, str]], url_expiry: int = 3600) -> List[Dict[str, Any]]:
    client = get_r2_client()
    prefix = f"{uid}/{video_id}/clips/"
    paginator = client.get_paginator("list_objects_v2")
    size_map: Dict[str, int] = {}
    keys: List[str] = []
    for page in paginator.paginate(Bucket=R2_BUCKET_NAME, Prefix=prefix):
        for obj in page.get("Contents", []):
            key = obj["Key"]
            size_map[key] = obj["Size"]
            keys.append(key)
    keys_set = set(keys)
    clips: List[Dict[str, Any]] = []
    for key in keys:
        if not key.lower().endswith(".mp4"):
            continue
        filename = key.split("/")[-1]
        size_bytes = size_map.get(key, 0)
        size_mb = size_bytes / (1024 * 1024) if size_bytes else 0.0
        thumb_key = key[:-4] + ".jpg"
        thumb_url = None
        if thumb_key in keys_set:
            thumb_url = generate_presigned_url(thumb_key, expires_in=url_expiry)
        title_text = filename
        description_text = ""
        try:
            parts = filename.split("_")
            if len(parts) >= 3 and parts[0] == "clip":
                clip_id = int(parts[2])
                if clip_id in highlights_map:
                    meta = highlights_map[clip_id]
                    title_text = meta.get("title", title_text)
                    description_text = meta.get("description", "")
        except Exception:
            pass
        url = generate_presigned_url(key, expires_in=url_expiry)
        clips.append(
            {
                "name": filename,
                "title": title_text,
                "description": description_text,
                "url": url,
                "thumbnail": thumb_url,
                "size": f"{size_mb:.1f} MB",
            }
        )
    clips.sort(key=lambda item: item["name"])
    return clips