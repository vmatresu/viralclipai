#!/bin/bash
# =============================================================================
# Build OpenCV with TUNED ISA Profile (AVX-512 dispatch)
# =============================================================================
# This profile is for pinned fleets with AVX-512 capable CPUs:
#   - Intel: Skylake-X, Cascade Lake, Ice Lake, Sapphire Rapids
#   - AMD: EPYC 7002/7003/7004 (Zen 2/3/4)
#
# IMPORTANT: Runtime CPU verification is REQUIRED when using this build.
# Use the CpuFeatures::verify_tuned_requirements() function before inference.
#
# Usage: ./build-tuned.sh
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Building OpenCV with TUNED profile (AVX-512 dispatch)..."
echo ""
echo "WARNING: This build requires AVX-512 capable CPUs!"
echo "Runtime verification is required to prevent SIGILL crashes."
echo ""

# Build the Docker image and extract artifacts
docker build \
    --target export \
    --build-arg ISA_PROFILE=tuned \
    --build-arg OPENCV_VERSION=4.12.0 \
    --build-arg OPENVINO_VERSION=2024.4 \
    -f "${SCRIPT_DIR}/Dockerfile.openvino" \
    -o type=local,dest="${SCRIPT_DIR}/artifacts" \
    "${SCRIPT_DIR}"

echo ""
echo "Build complete! Artifacts in: ${SCRIPT_DIR}/artifacts/"
ls -la "${SCRIPT_DIR}/artifacts/"
