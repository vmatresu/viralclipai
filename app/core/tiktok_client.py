from typing import Any, Dict

import httpx

from app.config import TIKTOK_API_BASE_URL, logger
from app.core.storage import generate_presigned_url


async def publish_clip_to_tiktok(settings: Dict[str, Any], s3_key: str, title: str, description: str) -> Dict[str, Any]:
    if not TIKTOK_API_BASE_URL:
        raise RuntimeError("TIKTOK_API_BASE_URL is not configured")
    access_token = settings.get("tiktok_access_token")
    account_id = settings.get("tiktok_account_id")
    if not access_token or not account_id:
        raise RuntimeError("TikTok settings are not configured for this user")
    video_url = generate_presigned_url(s3_key)
    if not video_url:
        raise RuntimeError("Failed to generate video URL for TikTok publishing")
    payload = {
        "access_token": access_token,
        "account_id": account_id,
        "video_url": video_url,
        "title": title,
        "description": description,
    }
    async with httpx.AsyncClient(timeout=60.0) as client:
        response = await client.post(TIKTOK_API_BASE_URL, json=payload)
        response.raise_for_status()
        return response.json()
