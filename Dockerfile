# Backend (FastAPI) multi-stage image

FROM python:3.11-slim AS base

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /app

# System deps: ffmpeg for video processing, build tools for any native wheels
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ffmpeg \
       build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install Python dependencies
COPY requirements.txt ./
RUN pip install --upgrade pip \
    && pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY app ./app
COPY prompt.txt ./prompt.txt

# Create non-root user
RUN useradd -m -u 1000 appuser \
    && chown -R appuser /app
USER appuser

# Development image (with autoreload)
FROM base AS dev
ENV ENVIRONMENT=development
EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000", "--reload"]

# Production image (gunicorn + uvicorn workers)
FROM base AS prod
ENV ENVIRONMENT=production

# Install gunicorn in production stage only
USER root
RUN pip install --no-cache-dir gunicorn \
    && chown -R appuser /app
USER appuser

EXPOSE 8000
CMD [
  "gunicorn",
  "-k", "uvicorn.workers.UvicornWorker",
  "-w", "4",
  "-b", "0.0.0.0:8000",
  "app.main:app"
]
