# =============================================================================
# Production Dockerfile - Multi-Architecture Rust Backend
# =============================================================================
# Multi-stage build optimized for:
# - Multi-arch support (ARM64 for M1 dev, AMD64 for Ubuntu prod)
# - Minimal image size with distroless/debian-slim runtime
# - Fast builds with cargo-chef dependency caching
# - Security: non-root user, minimal attack surface, no shell
# - Performance: LTO, single codegen unit, stripped binaries
#
# Stack: Rust + Firebase/Firestore + Cloudflare R2 + Redis (NO PostgreSQL)
#
# Build targets:
#   --target api-runtime    → Distroless API server (~30MB)
#   --target worker-runtime → Debian-slim with FFmpeg (~150MB)
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Chef Base - Install cargo-chef for dependency caching
# -----------------------------------------------------------------------------
FROM --platform=$BUILDPLATFORM rust:1.87-bookworm AS chef

# Install cargo-chef
RUN cargo install cargo-chef --locked

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: Planner - Generate dependency recipe (runs on build platform)
# -----------------------------------------------------------------------------
FROM chef AS planner

# Copy workspace manifests first (for better caching)
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/rust-toolchain.toml ./
COPY backend/crates ./crates

# Generate recipe.json - captures all dependencies
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage 3: Builder - Cross-compile for target architecture
# -----------------------------------------------------------------------------
FROM rust:1.87-bookworm AS builder

# Build arguments for multi-arch
ARG TARGETPLATFORM
ARG TARGETARCH

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-chef
RUN cargo install cargo-chef --locked

WORKDIR /app

# Copy dependency recipe from planner
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies ONLY (cached layer - only rebuilds if Cargo.toml/lock changes)
# This is the key optimization - dependencies are cached separately from source
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/rust-toolchain.toml ./
COPY backend/crates ./crates

# Build application with release optimizations
# LTO and single codegen unit are set in Cargo.toml [profile.release]
RUN cargo build --release --bin vclip-api --bin vclip-worker

# Strip binaries for smaller size (saves ~50-70%)
RUN strip target/release/vclip-api target/release/vclip-worker

# Verify binaries exist and show sizes
RUN ls -lah target/release/vclip-api target/release/vclip-worker

# -----------------------------------------------------------------------------
# Stage 4: API Runtime - Minimal distroless image
# -----------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS api-runtime

# OCI labels
LABEL org.opencontainers.image.title="ViralClip API" \
      org.opencontainers.image.description="Axum HTTP/WebSocket API server" \
      org.opencontainers.image.vendor="ViralClip AI" \
      org.opencontainers.image.source="https://github.com/viralclipai/viralclipai"

WORKDIR /app

# Copy SSL certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy API binary only
COPY --from=builder --chown=nonroot:nonroot /app/target/release/vclip-api /app/vclip-api

USER nonroot

# Production environment
ENV RUST_LOG=info,tower_http=info \
    RUST_BACKTRACE=1 \
    TZ=UTC

EXPOSE 8000

ENTRYPOINT ["/app/vclip-api"]

# -----------------------------------------------------------------------------
# Stage 5: Worker Runtime - Debian-slim with FFmpeg for video processing
# -----------------------------------------------------------------------------
FROM debian:12-slim AS worker-runtime

# OCI labels
LABEL org.opencontainers.image.title="ViralClip Worker" \
      org.opencontainers.image.description="Video processing worker with FFmpeg" \
      org.opencontainers.image.vendor="ViralClip AI"

# Install runtime dependencies only
RUN apt-get update && apt-get install -y --no-install-recommends \
        ffmpeg \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user
RUN groupadd -g 65532 appgroup && \
    useradd -u 65532 -g appgroup -d /app -s /usr/sbin/nologin appuser

WORKDIR /app

# Copy worker binary
COPY --from=builder --chown=appuser:appgroup /app/target/release/vclip-worker /app/vclip-worker

# Create temp directory for video processing
RUN mkdir -p /tmp/videos && chown appuser:appgroup /tmp/videos

USER appuser

# Production environment
ENV RUST_LOG=info,tower_http=info \
    RUST_BACKTRACE=1 \
    TZ=UTC \
    TMPDIR=/tmp/videos

# Health check - worker doesn't expose HTTP, check process
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD pgrep -x vclip-worker || exit 1

ENTRYPOINT ["/app/vclip-worker"]
