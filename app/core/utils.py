import urllib.parse as up
import hashlib

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
