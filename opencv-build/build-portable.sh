#!/bin/bash
# =============================================================================
# Build OpenCV with PORTABLE ISA Profile (AVX2 baseline)
# =============================================================================
# This profile is safe for deployment on any modern x86_64 CPU:
#   - Intel: Haswell (2013) and later
#   - AMD: Excavator (2015), Zen (2017) and later
#   - All major cloud providers (AWS, GCP, Azure)
#
# Usage: ./build-portable.sh
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Building OpenCV with PORTABLE profile (AVX2 baseline)..."
echo "This build will work on all modern x86_64 CPUs."
echo ""

# Build the Docker image and extract artifacts
docker build \
    --target export \
    --build-arg ISA_PROFILE=portable \
    --build-arg OPENCV_VERSION=4.12.0 \
    --build-arg OPENVINO_VERSION=2024.4 \
    -f "${SCRIPT_DIR}/Dockerfile.openvino" \
    -o type=local,dest="${SCRIPT_DIR}/artifacts" \
    "${SCRIPT_DIR}"

echo ""
echo "Build complete! Artifacts in: ${SCRIPT_DIR}/artifacts/"
ls -la "${SCRIPT_DIR}/artifacts/"
