import urllib.parse as up
import hashlib
import uuid
import subprocess
import logging
from datetime import datetime

logger = logging.getLogger(__name__)

def generate_run_id() -> str:
    """
    Generates a unique run ID for folder naming.
    Format: YYYYMMDD_HHMMSS_RANDOM
    """
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    short_uuid = str(uuid.uuid4())[:8]
    return f"{timestamp}_{short_uuid}"

def extract_youtube_id(url: str) -> str:
    """
    Robust extraction of YouTube ID from various URL formats.
    """
    parsed = up.urlparse(url)
    if parsed.netloc in {"youtu.be"}:
        return parsed.path.lstrip("/")
    
    if "youtube.com" in parsed.netloc:
        qs = up.parse_qs(parsed.query)
        if "v" in qs:
            return qs["v"][0]
        
        # Handle /shorts/, /embed/, /v/
        path_parts = [p for p in parsed.path.split("/") if p]
        if path_parts and path_parts[0] in {"shorts", "embed", "v"} and len(path_parts) > 1:
            return path_parts[1]

    # Fallback: deterministic hash for non-standard URLs
    return "video_" + str(abs(hash(url)))

def sanitize_filename(text: str, max_len: int = 60) -> str:
    text = text.lower().strip().replace(" ", "-")
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in text)
    return safe[:max_len] if safe else "clip"

def fetch_youtube_title(url: str) -> str | None:
    """
    Fetches the YouTube video title using yt-dlp without downloading the video.
    
    Args:
        url: YouTube video URL
        
    Returns:
        Video title as a string, or None if fetching fails
    """
    try:
        logger.info(f"Fetching title for {url} using yt-dlp...")
        result = subprocess.run(
            [
                "yt-dlp",
                "--print", "%(title)s",
                "--no-warnings",
                url,
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        title = result.stdout.strip()
        if title:
            logger.info(f"Fetched title: {title}")
            return title
        else:
            logger.warning(f"No title returned from yt-dlp for {url}")
            return None
    except subprocess.CalledProcessError as e:
        logger.warning(f"yt-dlp failed to fetch title: {e.stderr}")
        return None
    except subprocess.TimeoutExpired:
        logger.warning(f"yt-dlp timed out while fetching title for {url}")
        return None
    except Exception as e:
        logger.warning(f"Unexpected error fetching title: {e}")
        return None
