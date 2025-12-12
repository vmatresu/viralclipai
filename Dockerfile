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
#   --target worker-runtime → Ubuntu-slim with FFmpeg (~150MB)
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
FROM ubuntu:24.04 AS builder

# Build arguments for multi-arch
ARG TARGETPLATFORM
ARG TARGETARCH
ARG SERVICE_TYPE=api

# Install build dependencies and pre-built OpenCV 4.12.0
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        ca-certificates \
        curl \
        # OpenCV: Use pre-built 4.12.0 artifacts instead of apt packages
        # Clang/LLVM for opencv-rust bindgen
        clang \
        libclang-dev \
        # OpenCV runtime dependencies (needed for linking during build)
        libtbb12 \
        libwebp7 \
        libwebpdemux2 \
        libwebpmux3 \
        # FFmpeg libraries (OpenCV was built with ffmpeg support)
        libavcodec60 \
        libavformat60 \
        libavutil58 \
        libswscale7 \
        # Image codec libraries (required by opencv_imgcodecs)
        libpng16-16 \
        libtiff6 \
    && rm -rf /var/lib/apt/lists/*

# Install Rust 1.87 via rustup (matches rust:1.87 toolchain)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain 1.87.0
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy and extract pre-built OpenCV 4.12.0 artifacts (AMD64 for production)
COPY opencv-artifacts/opencv-4.12.0-ubuntu24.04-amd64.tar.gz /tmp/opencv.tar.gz
RUN cd /usr/local && \
    tar -xzf /tmp/opencv.tar.gz && \
    rm /tmp/opencv.tar.gz && \
    ldconfig

# Copy and extract pre-built ONNX Runtime 1.22.0 (for ort crate - AMD64 for production)
# Using pre-downloaded artifacts instead of download-binaries for reproducible builds
COPY onnxruntime-artifacts/ort-linux-x64.tgz /tmp/ort.tgz
RUN tar -xzf /tmp/ort.tgz -C /usr/local/lib --strip-components=1 && \
    rm /tmp/ort.tgz && \
    ldconfig

# Set ORT_LIB_LOCATION for the ort-sys crate build script
ENV ORT_LIB_LOCATION="/usr/local/lib"

# Install cargo-chef
RUN cargo install cargo-chef --locked

WORKDIR /app

# Copy dependency recipe from planner
COPY --from=planner /app/recipe.json recipe.json

# =============================================================================
# OPENCV LINKING CONFIGURATION
# =============================================================================
# Explicitly specify which OpenCV libraries to link against.
# This prevents the opencv-rust crate from using pkg-config/cmake auto-discovery
# which finds references to contrib modules (alphamat, barcode, cvv, hdf, viz)
# that we intentionally excluded in our custom OpenCV 4.12.0 build.
#
# This list includes only the modules we actually built and need:
# - Core modules: core, imgproc, imgcodecs, videoio, objdetect, dnn, calib3d, features2d, flann
# - Contrib modules we DID build: face, tracking, text, aruco, bgsegm, etc.
# =============================================================================
ENV OPENCV_LINK_LIBS="opencv_core,opencv_imgproc,opencv_imgcodecs,opencv_videoio,opencv_objdetect,opencv_dnn,opencv_calib3d,opencv_features2d,opencv_flann,opencv_photo,opencv_video,opencv_highgui,opencv_ml,opencv_stitching,opencv_aruco,opencv_bgsegm,opencv_bioinspired,opencv_ccalib,opencv_dnn_objdetect,opencv_dnn_superres,opencv_dpm,opencv_face,opencv_freetype,opencv_fuzzy,opencv_hfs,opencv_img_hash,opencv_intensity_transform,opencv_line_descriptor,opencv_mcc,opencv_optflow,opencv_phase_unwrapping,opencv_plot,opencv_quality,opencv_rapid,opencv_reg,opencv_saliency,opencv_shape,opencv_stereo,opencv_structured_light,opencv_superres,opencv_surface_matching,opencv_text,opencv_tracking,opencv_videostab,opencv_wechat_qrcode,opencv_ximgproc,opencv_xobjdetect,opencv_xphoto"

# Build dependencies ONLY (cached layer - only rebuilds if Cargo.toml/lock changes)
# This is the key optimization - dependencies are cached separately from source
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/rust-toolchain.toml ./
COPY backend/crates ./crates

# Build application with release optimizations
# LTO and single codegen unit are set in Cargo.toml [profile.release]
# Only build the binary needed for this service type
RUN if [ "$SERVICE_TYPE" = "api" ]; then \
        cargo build --release --bin vclip-api; \
    elif [ "$SERVICE_TYPE" = "worker" ]; then \
        cargo build --release --bin vclip-worker; \
    else \
        echo "Invalid SERVICE_TYPE: $SERVICE_TYPE" && exit 1; \
    fi

# Strip binaries for smaller size (saves ~50-70%)
RUN if [ "$SERVICE_TYPE" = "api" ]; then \
        strip target/release/vclip-api && ls -lah target/release/vclip-api; \
    elif [ "$SERVICE_TYPE" = "worker" ]; then \
        strip target/release/vclip-worker && ls -lah target/release/vclip-worker; \
    fi

# -----------------------------------------------------------------------------
# Stage 4: API Runtime - Ubuntu 24.04 slim (matches builder glibc version)
# -----------------------------------------------------------------------------
# Note: We use Ubuntu 24.04 instead of distroless because the binary is built
# with Ubuntu 24.04's glibc 2.39, which is not available in Debian 12 distroless.
FROM ubuntu:24.04 AS api-runtime

# OCI labels
LABEL org.opencontainers.image.title="ViralClip API" \
      org.opencontainers.image.description="Axum HTTP/WebSocket API server" \
      org.opencontainers.image.vendor="ViralClip AI" \
      org.opencontainers.image.source="https://github.com/viralclipai/viralclipai"

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user
RUN groupadd -g 65532 nonroot && \
    useradd -u 65532 -g nonroot -d /app -s /usr/sbin/nologin nonroot

WORKDIR /app

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
# Stage 5: Worker Runtime - Ubuntu-slim with FFmpeg for video processing
# -----------------------------------------------------------------------------
FROM ubuntu:24.04 AS worker-runtime

# OCI labels
LABEL org.opencontainers.image.title="ViralClip Worker" \
      org.opencontainers.image.description="Video processing worker with FFmpeg" \
      org.opencontainers.image.vendor="ViralClip AI"

# Install runtime dependencies only
RUN apt-get update && apt-get install -y --no-install-recommends \
        ffmpeg \
        ca-certificates \
        curl \
        python3 \
        python3-pip \
        # OpenCV: Use pre-built 4.12.0 runtime libraries
        libtbb12 \
        libwebp7 \
        libwebpdemux2 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Install yt-dlp for YouTube video downloads
RUN pip3 install --no-cache-dir --break-system-packages yt-dlp

# Install deno - required JavaScript runtime for yt-dlp YouTube extraction
# YouTube extraction without a JS runtime has been deprecated in yt-dlp
RUN apt-get update && apt-get install -y --no-install-recommends unzip \
    && curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/usr/local sh \
    && rm -rf /var/lib/apt/lists/*

# Copy and extract pre-built OpenCV 4.12.0 runtime libraries (AMD64 for production)
COPY opencv-artifacts/opencv-4.12.0-ubuntu24.04-amd64.tar.gz /tmp/opencv.tar.gz
RUN cd /usr/local && \
    tar -xzf /tmp/opencv.tar.gz && \
    rm /tmp/opencv.tar.gz && \
    ldconfig

# Copy and extract pre-built ONNX Runtime 1.22.0 runtime libraries
COPY onnxruntime-artifacts/ort-linux-x64.tgz /tmp/ort.tgz
RUN tar -xzf /tmp/ort.tgz -C /usr/local/lib --strip-components=1 && \
    rm /tmp/ort.tgz && \
    ldconfig

# Copy YuNet face detection models
COPY backend/models/face_detection/yunet /app/backend/models/face_detection/yunet

# Create non-root user
# Make this idempotent and avoid failing if UID/GID 65532 already exist in the base image.
RUN getent group appgroup >/dev/null 2>&1 || groupadd -g 65532 appgroup && \
    id -u appuser >/dev/null 2>&1 || useradd -u 65532 -g appgroup -d /app -s /usr/sbin/nologin appuser

WORKDIR /app

# Copy worker binary
COPY --from=builder --chown=appuser:appgroup /app/target/release/vclip-worker /app/vclip-worker

# Create directories for video processing and yt-dlp cache
# yt-dlp needs write access to .cache for challenge solver and other caches
RUN mkdir -p /tmp/videos /app/.cache && chown -R appuser:appgroup /tmp/videos /app/.cache

# Create placeholder for youtube-cookies.txt with correct ownership
# The actual file will be mounted via docker-compose volume
RUN touch /app/youtube-cookies.txt && chown appuser:appgroup /app/youtube-cookies.txt

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
