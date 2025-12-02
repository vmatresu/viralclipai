#!/bin/bash
# =============================================================================
# Docker Build Script with BuildKit Optimizations
# =============================================================================
# This script provides optimized Docker builds with BuildKit caching
# =============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DOCKER_BUILDKIT=1
export DOCKER_BUILDKIT

# Get version info
VERSION="${VERSION:-$(git describe --tags --always --dirty 2>/dev/null || echo 'dev')}"
BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
VCS_REF="$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"

# Functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Build function
build_image() {
    local service=$1
    local dockerfile=$2
    local context=$3
    local target=$4
    local extra_args="${5:-}"

    log_info "Building ${service} image..."
    
    docker build \
        --target "${target}" \
        --build-arg BUILD_DATE="${BUILD_DATE}" \
        --build-arg VCS_REF="${VCS_REF}" \
        --build-arg VERSION="${VERSION}" \
        ${extra_args} \
        --tag "vidclips-${service}:${VERSION}" \
        --tag "vidclips-${service}:latest" \
        --progress=plain \
        --file "${dockerfile}" \
        "${context}" || {
        log_error "Failed to build ${service}"
        exit 1
    }
    
    log_info "Successfully built vidclips-${service}:${VERSION}"
}

# Main
main() {
    local service="${1:-all}"
    local target="${2:-prod}"

    log_info "Starting Docker build process..."
    log_info "Version: ${VERSION}"
    log_info "Build Date: ${BUILD_DATE}"
    log_info "VCS Ref: ${VCS_REF}"

    case "${service}" in
        api)
            build_image "api" "Dockerfile" "." "${target}"
            ;;
        web)
            local web_args="--build-arg NEXT_PUBLIC_API_BASE_URL=${NEXT_PUBLIC_API_BASE_URL:-http://api:8000}"
            build_image "web" "web/Dockerfile" "./web" "${target}" "${web_args}"
            ;;
        all)
            build_image "api" "Dockerfile" "." "${target}"
            local web_args="--build-arg NEXT_PUBLIC_API_BASE_URL=${NEXT_PUBLIC_API_BASE_URL:-http://api:8000}"
            build_image "web" "web/Dockerfile" "./web" "${target}" "${web_args}"
            ;;
        *)
            log_error "Unknown service: ${service}"
            echo "Usage: $0 [api|web|all] [prod|dev|test]"
            exit 1
            ;;
    esac

    log_info "Build completed successfully!"
}

# Run main function
main "$@"

