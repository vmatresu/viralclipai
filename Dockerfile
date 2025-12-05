# =============================================================================
# Production-Grade Rust Backend Dockerfile
# =============================================================================
# Multi-stage build optimized for:
# - Minimal image size with distroless runtime
# - Fast builds with cargo-chef dependency caching
# - Security with non-root user and minimal attack surface
# - Performance with LTO and optimized release builds
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Chef Planner - Prepare dependency recipe
# -----------------------------------------------------------------------------
FROM rust:1.82-alpine AS chef

# Install cargo-chef for optimal dependency caching
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static && \
    cargo install cargo-chef --locked

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: Planner - Generate dependency recipe
# -----------------------------------------------------------------------------
FROM chef AS planner

# Copy workspace manifests
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/rust-toolchain.toml ./
COPY backend/crates ./crates

# Generate recipe.json for dependency caching
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage 3: Builder - Build dependencies and application
# -----------------------------------------------------------------------------
FROM chef AS builder

# Copy dependency recipe
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies (cached layer - only rebuilds if dependencies change)
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/rust-toolchain.toml ./
COPY backend/crates ./crates

# Build application with optimizations (LTO, single codegen unit, stripped)
RUN cargo build --release --bins && \
    strip target/release/vclip-api target/release/vclip-worker

# -----------------------------------------------------------------------------
# Stage 4: Runtime - Minimal production image for API
# -----------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# OCI labels for metadata
LABEL org.opencontainers.image.title="ViralClip AI Rust Backend" \
      org.opencontainers.image.description="High-performance Rust backend for AI-powered video clip generation" \
      org.opencontainers.image.vendor="Viral Clip AI" \
      org.opencontainers.image.licenses="Proprietary"

# Copy SSL certificates for HTTPS
COPY --from=builder --chown=nonroot:nonroot /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy compiled binaries
COPY --from=builder --chown=nonroot:nonroot /app/target/release/vclip-api /app/vclip-api
COPY --from=builder --chown=nonroot:nonroot /app/target/release/vclip-worker /app/vclip-worker

WORKDIR /app
USER nonroot

# Environment variables for production
ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    TZ=UTC

EXPOSE 8000

# Default to API server (can be overridden in docker-compose)
ENTRYPOINT ["/app/vclip-api"]

# -----------------------------------------------------------------------------
# Stage 5: Runtime with FFmpeg - For worker containers
# -----------------------------------------------------------------------------
FROM debian:12-slim AS runtime-ffmpeg

# Install FFmpeg and CA certificates
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ffmpeg \
        ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -g 1000 appgroup && \
    useradd -u 1000 -g appgroup -d /app -s /sbin/nologin appuser

# Copy compiled binaries
COPY --from=builder --chown=appuser:appgroup /app/target/release/vclip-api /app/vclip-api
COPY --from=builder --chown=appuser:appgroup /app/target/release/vclip-worker /app/vclip-worker

WORKDIR /app
USER appuser

# Environment variables for production
ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    TZ=UTC \
    PATH="/usr/bin:${PATH}"

# Verify FFmpeg is available
RUN ffmpeg -version

EXPOSE 8000

# Default to worker (can be overridden)
ENTRYPOINT ["/app/vclip-worker"]

# -----------------------------------------------------------------------------
# Stage 6: Development - Full toolchain with hot reload
# -----------------------------------------------------------------------------
FROM rust:1.82-alpine AS dev

# Install development dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    ffmpeg \
    git \
    curl && \
    cargo install cargo-watch --locked

WORKDIR /app

# Development environment
ENV RUST_LOG=debug \
    RUST_BACKTRACE=full \
    CARGO_INCREMENTAL=1

# Hot reload with cargo-watch
CMD ["cargo", "watch", "-x", "run --bin vclip-api"]
