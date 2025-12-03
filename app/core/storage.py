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
        style = None
        
        # Extract style from filename: clip_XX_XX_title_style.mp4
        # Style is the part before .mp4 after the last underscore
        # Note: Some styles like "intelligent_split" contain underscores, so we need to check
        # for known styles by trying to match them from the end of the filename
        # The style is always preceded by an underscore in the filename format
        try:
            if filename.endswith(".mp4"):
                name_without_ext = filename[:-4]  # Remove .mp4
                # Known styles ordered by length (longest first) to avoid partial matches
                # This ensures "intelligent_split" is matched before "split"
                known_styles = ["intelligent_split", "left_focus", "right_focus", "intelligent", "split", "original"]
                # Try to match each style by checking if filename ends with "_style"
                # We only check for "_style" pattern since the filename format always has underscore before style
                for known_style in known_styles:
                    if name_without_ext.endswith(f"_{known_style}"):
                        style = known_style
                        break
        except Exception:
            pass
        
        # Extract title and description from highlights map
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
        # Use relative URL for video clips to go through backend proxy with CORS headers
        # This ensures proper CORS handling for video playback
        url = f"/api/videos/{video_id}/clips/{filename}"
        clip_data = {
            "name": filename,
            "title": title_text,
            "description": description_text,
            "url": url,
            "thumbnail": thumb_url,
            "size": f"{size_mb:.1f} MB",
        }
        if style:
            clip_data["style"] = style
        clips.append(clip_data)
    clips.sort(key=lambda item: item["name"])
    return clips


def delete_video_files(uid: str, video_id: str) -> int:
    """
    Delete all files associated with a video from R2 storage.
    
    Args:
        uid: User ID
        video_id: Video ID
        
    Returns:
        Number of objects deleted
        
    Raises:
        ClientError: If deletion fails
    """
    client = get_r2_client()
    prefix = f"{uid}/{video_id}/"
    
    # List all objects with this prefix
    paginator = client.get_paginator("list_objects_v2")
    objects_to_delete: List[Dict[str, str]] = []
    
    for page in paginator.paginate(Bucket=R2_BUCKET_NAME, Prefix=prefix):
        for obj in page.get("Contents", []):
            objects_to_delete.append({"Key": obj["Key"]})
    
    if not objects_to_delete:
        logger.info("No files found to delete for video %s/%s", uid, video_id)
        return 0
    
    # Delete objects in batches (R2 supports up to 1000 objects per request)
    deleted_count = 0
    batch_size = 1000
    
    for i in range(0, len(objects_to_delete), batch_size):
        batch = objects_to_delete[i:i + batch_size]
        try:
            response = client.delete_objects(
                Bucket=R2_BUCKET_NAME,
                Delete={"Objects": batch, "Quiet": True}
            )
            # Count successful deletions
            deleted_count += len(batch)
            if response.get("Errors"):
                logger.warning(
                    "Some objects failed to delete for video %s/%s: %s",
                    uid, video_id, response["Errors"]
                )
        except ClientError as exc:
            logger.error("Failed to delete objects for video %s/%s: %s", uid, video_id, exc)
            raise
    
    logger.info("Deleted %d objects for video %s/%s", deleted_count, uid, video_id)
    return deleted_count


def delete_clip(uid: str, video_id: str, clip_name: str) -> int:
    """
    Delete a single clip and its thumbnail from R2 storage.
    
    Args:
        uid: User ID
        video_id: Video ID
        clip_name: Name of the clip file (e.g., "clip_0_split.mp4")
        
    Returns:
        Number of objects deleted (should be 1 or 2: clip + optional thumbnail)
        
    Raises:
        ClientError: If deletion fails
        ValueError: If clip_name is invalid
    """
    from app.core.security.validation import validate_clip_name, ValidationError
    
    # Validate clip_name to prevent path traversal
    try:
        clip_name = validate_clip_name(clip_name)
    except ValidationError as e:
        logger.error("Invalid clip name %s: %s", clip_name, e)
        raise ValueError(f"Invalid clip name: {e.message}")
    
    client = get_r2_client()
    
    # Construct keys for clip and thumbnail
    clip_key = f"{uid}/{video_id}/clips/{clip_name}"
    # Thumbnail is typically the same name but with .jpg extension
    thumbnail_key = clip_key.rsplit(".", 1)[0] + ".jpg"
    
    # Always attempt to delete both clip and thumbnail - deletion is idempotent
    objects_to_delete = [
        {"Key": clip_key},
        {"Key": thumbnail_key}
    ]

    # Delete objects
    try:
        response = client.delete_objects(
            Bucket=R2_BUCKET_NAME,
            Delete={"Objects": objects_to_delete, "Quiet": True}
        )

        # Count successful deletions (R2 returns deleted objects in response)
        deleted_objects = response.get("Deleted", [])
        deleted_count = len(deleted_objects)

        if response.get("Errors"):
            logger.warning(
                "Some objects failed to delete for clip %s/%s/%s: %s",
                uid, video_id, clip_name, response["Errors"]
            )

        logger.info(
            "Deleted %d object(s) for clip %s/%s/%s",
            deleted_count, uid, video_id, clip_name
        )
        return deleted_count
    except ClientError as exc:
        logger.error("Failed to delete clip %s/%s/%s: %s", uid, video_id, clip_name, exc)
        raise