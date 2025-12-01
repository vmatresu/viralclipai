import os
import logging
from pathlib import Path

# Base Paths
APP_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = APP_DIR.parent

# Resources
VIDEOS_DIR = PROJECT_ROOT / "videos"  # local scratch working directory
PROMPT_PATH = PROJECT_ROOT / "prompt.txt"
STATIC_DIR = APP_DIR / "static"

# Logging Setup
LOG_LEVEL = os.getenv("LOG_LEVEL", "DEBUG").upper()
LOG_FORMAT = "%(asctime)s [%(levelname)s] %(message)s"

logging.basicConfig(
    level=getattr(logging, LOG_LEVEL),
    format=LOG_FORMAT,
    handlers=[
        logging.FileHandler("debug.log"),
        logging.StreamHandler()
    ]
)

logger = logging.getLogger("vidclips")

# Ensure directories exist
VIDEOS_DIR.mkdir(parents=True, exist_ok=True)


def _split_csv(value: str) -> list[str]:
    return [item.strip() for item in value.split(",") if item.strip()]


# AWS / S3
AWS_REGION = os.getenv("AWS_REGION", "us-east-1")
S3_BUCKET_NAME = os.getenv("S3_BUCKET_NAME", "")

# TikTok API
TIKTOK_API_BASE_URL = os.getenv("TIKTOK_API_BASE_URL", "")

# Security / domains
ALLOWED_HOSTS = _split_csv(
    os.getenv(
        "ALLOWED_HOSTS",
        "localhost,127.0.0.1,viralvideoai.io,www.viralvideoai.io",
    )
)

CORS_ORIGINS = _split_csv(
    os.getenv(
        "CORS_ORIGINS",
        "http://localhost:8000,https://viralvideoai.io,https://www.viralvideoai.io",
    )
)