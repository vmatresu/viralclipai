"""
Security Constants

Centralized constants for security module.
"""

# Allowed video URL patterns (YouTube, Vimeo, etc.)
ALLOWED_VIDEO_HOSTS = frozenset({
    "youtube.com",
    "www.youtube.com",
    "youtu.be",
    "m.youtube.com",
    "vimeo.com",
    "www.vimeo.com",
    "player.vimeo.com",
})

# Maximum lengths for user inputs
MAX_URL_LENGTH = 2048
MAX_PROMPT_LENGTH = 10000
MAX_TITLE_LENGTH = 500
MAX_DESCRIPTION_LENGTH = 5000

# Rate limiting defaults
DEFAULT_RATE_LIMIT = 60  # requests per window
DEFAULT_RATE_WINDOW = 60  # seconds
WEBSOCKET_RATE_LIMIT = 10  # connections per window
WEBSOCKET_RATE_WINDOW = 60  # seconds

# Request ID header
REQUEST_ID_HEADER = "X-Request-ID"

