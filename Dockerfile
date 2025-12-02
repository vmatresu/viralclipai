# =============================================================================
# Multi-Stage Production Dockerfile for FastAPI Backend
# =============================================================================
# Best Practices:
# - Multi-stage builds for minimal final image
# - Layer caching optimization
# - Security hardening (non-root, minimal packages)
# - Build-time arguments for flexibility
# - Health checks and proper signal handling
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Base - Common dependencies and configuration
# -----------------------------------------------------------------------------
# Using Alpine for smaller image size (~5MB vs ~50MB) and faster builds
FROM python:3.11-alpine AS base

# Build arguments
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION=dev

# Labels for metadata and traceability
LABEL org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.authors="Viral Clip AI" \
      org.opencontainers.image.url="https://viralvideoai.io" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.vendor="Viral Clip AI" \
      org.opencontainers.image.title="Viral Clip AI Backend" \
      org.opencontainers.image.description="FastAPI backend for video clip generation" \
      maintainer="Viral Clip AI Team"

# Security: Prevent Python from writing bytecode and buffer stdout/stderr
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DEFAULT_TIMEOUT=100 \
    # Set Python path
    PYTHONPATH=/app \
    # Security: Disable pip version check
    PIP_ROOT_USER_ACTION=ignore

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: System Dependencies - Install system packages
# -----------------------------------------------------------------------------
FROM base AS system-deps

# Install system dependencies in a single layer for better caching
# Alpine uses apk instead of apt-get - much faster and smaller (~5MB base vs ~50MB)
# Build dependencies are installed separately and removed after pip install
RUN apk add --no-cache \
        # Video processing (critical)
        ffmpeg \
        # SSL/TLS certificates
        ca-certificates \
        # Process management
        procps \
        # Network utilities for health checks
        curl \
        && \
    # Install build dependencies in virtual package (will be removed later)
    apk add --no-cache --virtual .build-deps \
        gcc \
        musl-dev \
        libffi-dev \
        openssl-dev \
        && \
    # Verify ffmpeg installation
    ffmpeg -version | head -n 1

# -----------------------------------------------------------------------------
# Stage 3: Python Dependencies - Install Python packages
# -----------------------------------------------------------------------------
FROM system-deps AS python-deps

# Copy only requirements first for better layer caching
COPY requirements.txt /tmp/requirements.txt

# Install Python dependencies in a separate layer
# This layer will be cached unless requirements.txt changes
RUN pip install --upgrade pip setuptools wheel && \
    pip install --no-cache-dir -r /tmp/requirements.txt && \
    # Verify critical packages
    python -c "import fastapi; import uvicorn; import google.generativeai" && \
    # Remove build dependencies virtual package to reduce image size (Alpine optimization)
    apk del --no-cache .build-deps && \
    # Clean up pip cache
    rm -rf ~/.cache/pip /tmp/requirements.txt

# -----------------------------------------------------------------------------
# Stage 4: Application Code - Copy application files
# -----------------------------------------------------------------------------
FROM python-deps AS app-code

# Copy application code (this layer changes frequently)
COPY app/ ./app/
COPY prompt.txt ./prompt.txt
# Copy credentials if present (best effort for dev/prod build context)
# Copy to root location to match volume mount structure in dev mode
COPY firebase-credentials.json* ./firebase-credentials.json

# Verify application structure
RUN python -c "from app.main import app; print('Application loaded successfully')" || exit 1

# -----------------------------------------------------------------------------
# Stage 5: Production Base - Final base with security hardening
# -----------------------------------------------------------------------------
FROM app-code AS prod-base

# Create non-root user with specific UID/GID for consistency
# Alpine uses addgroup/adduser instead of groupadd/useradd
RUN addgroup -g 1000 -S appgroup && \
    adduser -u 1000 -S -G appgroup -h /app -s /sbin/nologin appuser && \
    # Create necessary directories with proper permissions
    mkdir -p /app/logs /app/videos /app/static && \
    chown -R appuser:appgroup /app && \
    # Set proper permissions
    chmod -R 755 /app && \
    chmod -R 700 /app/logs /app/videos

# Switch to non-root user
USER appuser

# Health check script (lightweight, no dependencies)
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health', timeout=5)" || exit 1

# Expose application port
EXPOSE 8000

# -----------------------------------------------------------------------------
# Stage 6: Development - Development image with hot reload
# -----------------------------------------------------------------------------
FROM prod-base AS dev

ENV ENVIRONMENT=development \
    PYTHONUNBUFFERED=1

# Development command with auto-reload
CMD ["uvicorn", "app.main:app", \
     "--host", "0.0.0.0", \
     "--port", "8000", \
     "--reload", \
     "--reload-dir", "/app/app", \
     "--log-level", "debug"]

# -----------------------------------------------------------------------------
# Stage 7: Production - Production image with Gunicorn
# -----------------------------------------------------------------------------
FROM prod-base AS prod

ENV ENVIRONMENT=production \
    PYTHONUNBUFFERED=1 \
    # Gunicorn settings
    GUNICORN_WORKERS=4 \
    GUNICORN_TIMEOUT=120 \
    GUNICORN_KEEPALIVE=5 \
    GUNICORN_MAX_REQUESTS=10000 \
    GUNICORN_MAX_REQUESTS_JITTER=1000

# Install gunicorn in production (switch to root temporarily)
USER root
RUN pip install --no-cache-dir gunicorn && \
    rm -rf ~/.cache/pip && \
    chown -R appuser:appgroup /app
USER appuser

# Production command with optimized Gunicorn settings
# Using environment variables for flexibility
CMD ["sh", "-c", \
     "gunicorn -k uvicorn.workers.UvicornWorker \
     -w ${GUNICORN_WORKERS:-4} \
     -b 0.0.0.0:8000 \
     --timeout ${GUNICORN_TIMEOUT:-120} \
     --keep-alive ${GUNICORN_KEEPALIVE:-5} \
     --max-requests ${GUNICORN_MAX_REQUESTS:-10000} \
     --max-requests-jitter ${GUNICORN_MAX_REQUESTS_JITTER:-1000} \
     --limit-request-line 4094 \
     --limit-request-fields 100 \
     --limit-request-field_size 8190 \
     --access-logfile - \
     --error-logfile - \
     --log-level info \
     --capture-output \
     --enable-stdio-inheritance \
     --preload \
     app.main:app"]

# -----------------------------------------------------------------------------
# Stage 8: Test - Testing image with test dependencies
# -----------------------------------------------------------------------------
FROM python-deps AS test

# Install test dependencies (if you have a requirements-test.txt)
# COPY requirements-test.txt /tmp/requirements-test.txt
# RUN pip install --no-cache-dir -r /tmp/requirements-test.txt

COPY app/ ./app/
COPY prompt.txt ./prompt.txt

# Run tests
# CMD ["pytest", "-v", "--cov=app", "--cov-report=term-missing"]
