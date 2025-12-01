import os
import logging
from pathlib import Path

# Base Paths
APP_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = APP_DIR.parent

# Resources
VIDEOS_DIR = PROJECT_ROOT / "videos"  # local scratch working directory
PROMPT_PATH = PROJECT_ROOT / "prompt.txt"
TEMPLATES_DIR = APP_DIR / "templates"
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

# AWS / S3
AWS_REGION = os.getenv("AWS_REGION", "us-east-1")
S3_BUCKET_NAME = os.getenv("S3_BUCKET_NAME", "")

# TikTok API
TIKTOK_API_BASE_URL = os.getenv("TIKTOK_API_BASE_URL", "")

# Firebase Web SDK configuration for frontend
FIREBASE_WEB_CONFIG = {
    "apiKey": os.getenv("FIREBASE_WEB_API_KEY", ""),
    "authDomain": os.getenv("FIREBASE_WEB_AUTH_DOMAIN", ""),
    "projectId": os.getenv("FIREBASE_WEB_PROJECT_ID", ""),
    "storageBucket": os.getenv("FIREBASE_WEB_STORAGE_BUCKET", ""),
    "messagingSenderId": os.getenv("FIREBASE_WEB_MESSAGING_SENDER_ID", ""),
    "appId": os.getenv("FIREBASE_WEB_APP_ID", ""),
}