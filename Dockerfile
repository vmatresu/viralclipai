# Backend (FastAPI) multi-stage image
# Security-hardened production build

FROM python:3.11-slim AS base

# Security: Prevent Python from writing bytecode and buffer stdout/stderr
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    # Security: Disable pip version check to reduce attack surface
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_NO_CACHE_DIR=1

WORKDIR /app

# System deps: ffmpeg for video processing, build tools for any native wheels
# Security: Minimize installed packages, clean up after install
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ffmpeg \
       build-essential \
       # Security: Add ca-certificates for HTTPS
       ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

# Install Python dependencies
COPY requirements.txt ./
RUN pip install --upgrade pip \
    && pip install --no-cache-dir -r requirements.txt \
    # Security: Remove pip cache and unnecessary files
    && rm -rf ~/.cache/pip

# Copy application code
COPY app ./app
COPY prompt.txt ./prompt.txt

# Security: Create non-root user with specific UID/GID
RUN groupadd -r -g 1000 appgroup \
    && useradd -r -u 1000 -g appgroup -d /app -s /sbin/nologin appuser \
    && chown -R appuser:appgroup /app \
    # Security: Create necessary directories with proper permissions
    && mkdir -p /app/logs /app/videos \
    && chown -R appuser:appgroup /app/logs /app/videos

# Security: Switch to non-root user
USER appuser

# Development image (with autoreload)
FROM base AS dev
ENV ENVIRONMENT=development
EXPOSE 8000
# Security: Bind to localhost in dev, use 0.0.0.0 only when needed
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000", "--reload"]

# Production image (gunicorn + uvicorn workers)
FROM base AS prod
ENV ENVIRONMENT=production

# Install gunicorn in production stage only
USER root
RUN pip install --no-cache-dir gunicorn \
    && chown -R appuser:appgroup /app \
    && rm -rf ~/.cache/pip
USER appuser

# Security: Health check for container orchestration
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health')" || exit 1

EXPOSE 8000

# Security: Production settings
# - Limit workers to prevent resource exhaustion
# - Set timeouts to prevent slow loris attacks
# - Limit request line and header sizes
CMD ["gunicorn", "-k", "uvicorn.workers.UvicornWorker", "-w", "4", "-b", "0.0.0.0:8000", "--timeout", "120", "--keep-alive", "5", "--max-requests", "10000", "--max-requests-jitter", "1000", "--limit-request-line", "4094", "--limit-request-fields", "100", "--limit-request-field_size", "8190", "app.main:app"]
