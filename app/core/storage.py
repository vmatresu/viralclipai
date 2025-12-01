import json
from pathlib import Path
from typing import Any, Dict, List

import boto3
from botocore.exceptions import ClientError

from app.config import AWS_REGION, S3_BUCKET_NAME, logger

_s3_client = None


def get_s3_client():
    global _s3_client
    if _s3_client is None:
        _s3_client = boto3.client("s3", region_name=AWS_REGION)
    return _s3_client


def upload_file(path: Path, key: str, content_type: str) -> None:
    client = get_s3_client()
    extra_args: Dict[str, Any] = {"ContentType": content_type}
    client.upload_file(str(path), S3_BUCKET_NAME, key, ExtraArgs=extra_args)


def generate_presigned_url(key: str, expires_in: int = 3600) -> str:
    client = get_s3_client()
    try:
        url = client.generate_presigned_url(
            "get_object",
            Params={"Bucket": S3_BUCKET_NAME, "Key": key},
            ExpiresIn=expires_in,
        )
        return url
    except ClientError as exc:
        logger.error("Failed to generate presigned URL for %s: %s", key, exc)
        return ""


def load_highlights(uid: str, video_id: str) -> Dict[str, Any]:
    client = get_s3_client()
    key = f"{uid}/{video_id}/highlights.json"
    try:
        obj = client.get_object(Bucket=S3_BUCKET_NAME, Key=key)
    except ClientError as exc:
        logger.error("Failed to load highlights for %s/%s: %s", uid, video_id, exc)
        return {}
    body = obj["Body"].read()
    try:
        return json.loads(body)
    except Exception:
        return {}


def list_clips_with_metadata(uid: str, video_id: str, highlights_map: Dict[int, Dict[str, str]], url_expiry: int = 3600) -> List[Dict[str, Any]]:
    client = get_s3_client()
    prefix = f"{uid}/{video_id}/clips/"
    paginator = client.get_paginator("list_objects_v2")
    size_map: Dict[str, int] = {}
    keys: List[str] = []
    for page in paginator.paginate(Bucket=S3_BUCKET_NAME, Prefix=prefix):
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
