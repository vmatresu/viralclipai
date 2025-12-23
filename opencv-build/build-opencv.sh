#!/bin/bash
# =============================================================================
# OpenCV Build Script with OpenVINO and ISA Profile Support
# =============================================================================
# Usage: ./build-opencv.sh <ISA_PROFILE> <OPENCV_VERSION> <OPENVINO_VERSION>
#
# ISA Profiles:
#   portable - AVX2 baseline, works on all modern x86_64 CPUs (2013+)
#   tuned    - AVX-512 with runtime dispatch, for pinned fleets
#
# This script configures and builds OpenCV with:
#   - OpenVINO backend for accelerated DNN inference
#   - TBB for parallel processing
#   - IPP for optimized image processing (Intel CPUs)
#   - Minimal footprint (no GUI, no Python, no tests)
# =============================================================================

set -euo pipefail

ISA_PROFILE="${1:-portable}"
OPENCV_VERSION="${2:-4.12.0}"
OPENVINO_VERSION="${3:-2024.4}"

echo "=============================================="
echo "OpenCV Build Configuration"
echo "=============================================="
echo "OpenCV Version:   ${OPENCV_VERSION}"
echo "OpenVINO Version: ${OPENVINO_VERSION}"
echo "ISA Profile:      ${ISA_PROFILE}"
echo "=============================================="

# Source OpenVINO environment
if [ -f /opt/intel/openvino/setupvars.sh ]; then
    source /opt/intel/openvino/setupvars.sh
elif [ -f /usr/share/openvino/setupvars.sh ]; then
    source /usr/share/openvino/setupvars.sh
fi

# Determine OpenVINO CMake directory (hard fail if missing).
OPENVINO_CMAKE_DIR=""
OPENVINO_CONFIG=""

for dir in \
    "/opt/intel/openvino/runtime/cmake" \
    "/opt/intel/openvino/runtime/cmake/openvino" \
    "/usr/lib/x86_64-linux-gnu/cmake/openvino" \
    "/usr/share/openvino/cmake" \
    "/usr/local/lib/cmake/openvino" \
    "/usr/lib/cmake/openvino${OPENVINO_VERSION}" \
    "/usr/lib/cmake/openvino${OPENVINO_VERSION}.0"; do
    if [ -f "${dir}/OpenVINOConfig.cmake" ]; then
        OPENVINO_CONFIG="${dir}/OpenVINOConfig.cmake"
        break
    fi
done

if [ -z "${OPENVINO_CONFIG}" ]; then
    OPENVINO_CONFIG="$(find /opt/intel /usr /usr/local -type f -name OpenVINOConfig.cmake 2>/dev/null | head -n1 || true)"
fi

if [ -z "${OPENVINO_CONFIG}" ]; then
    echo "ERROR: OpenVINOConfig.cmake not found. Ensure OpenVINO dev packages are installed."
    exit 1
fi

OPENVINO_CMAKE_DIR="$(dirname "${OPENVINO_CONFIG}")"
echo "Found OpenVINO CMake at: ${OPENVINO_CMAKE_DIR}"
WITH_OPENVINO="ON"

# =============================================================================
# ISA Profile Configuration
# =============================================================================
case "${ISA_PROFILE}" in
    portable)
        # PORTABLE: Safe for all modern x86_64 CPUs (Haswell 2013+, AMD Zen 2017+)
        CPU_BASELINE="AVX2"
        CPU_DISPATCH=""
        ENABLE_AVX512="OFF"
        echo "Using PORTABLE profile: AVX2 baseline, no AVX-512"
        ;;
    tuned)
        # TUNED: For pinned fleets with AVX-512 support (Skylake-X, Ice Lake, EPYC)
        CPU_BASELINE="AVX2"
        CPU_DISPATCH="AVX512_SKX;AVX512_ICL"
        ENABLE_AVX512="ON"
        echo "Using TUNED profile: AVX2 baseline with AVX-512 dispatch"
        ;;
    *)
        echo "ERROR: Unknown ISA profile: ${ISA_PROFILE}"
        echo "Valid profiles: portable, tuned"
        exit 1
        ;;
esac

# =============================================================================
# CMake Configuration
# =============================================================================
cmake -G Ninja ../opencv \
    -D CMAKE_BUILD_TYPE=Release \
    -D CMAKE_INSTALL_PREFIX=/usr/local \
    -D OPENCV_EXTRA_MODULES_PATH=../opencv_contrib/modules \
    \
    `# ================================================================` \
    `# OpenVINO Integration (CRITICAL for DNN acceleration)` \
    `# ================================================================` \
    -D WITH_OPENVINO=${WITH_OPENVINO} \
    ${OPENVINO_CMAKE_DIR:+-D OpenVINO_DIR=${OPENVINO_CMAKE_DIR}} \
    \
    `# ================================================================` \
    `# CPU ISA Configuration` \
    `# ================================================================` \
    -D CPU_BASELINE=${CPU_BASELINE} \
    -D CPU_DISPATCH="${CPU_DISPATCH}" \
    -D ENABLE_AVX2=ON \
    -D ENABLE_AVX512=${ENABLE_AVX512} \
    \
    `# ================================================================` \
    `# Threading: TBB (Intel Threading Building Blocks)` \
    `# ================================================================` \
    -D WITH_TBB=ON \
    -D WITH_OPENMP=OFF \
    -D WITH_PTHREADS_PF=ON \
    \
    `# ================================================================` \
    `# Intel Performance Primitives (IPP)` \
    `# ================================================================` \
    -D WITH_IPP=ON \
    -D BUILD_IPP_IW=ON \
    \
    `# ================================================================` \
    `# FFmpeg for Video I/O` \
    `# ================================================================` \
    -D WITH_FFMPEG=ON \
    \
    `# ================================================================` \
    `# Image Codecs` \
    `# ================================================================` \
    -D WITH_PNG=ON \
    -D WITH_TIFF=ON \
    -D WITH_WEBP=ON \
    -D WITH_JPEG=ON \
    \
    `# ================================================================` \
    `# DNN Module (REQUIRED for YuNet)` \
    `# ================================================================` \
    -D BUILD_opencv_dnn=ON \
    \
    `# ================================================================` \
    `# Object Detection (REQUIRED for FaceDetectorYN)` \
    `# ================================================================` \
    -D BUILD_opencv_objdetect=ON \
    \
    `# ================================================================` \
    `# Face Module from contrib (for face-specific algorithms)` \
    `# ================================================================` \
    -D BUILD_opencv_face=ON \
    \
    `# ================================================================` \
    `# Core Modules (required dependencies)` \
    `# ================================================================` \
    -D BUILD_opencv_core=ON \
    -D BUILD_opencv_imgproc=ON \
    -D BUILD_opencv_imgcodecs=ON \
    -D BUILD_opencv_videoio=ON \
    -D BUILD_opencv_calib3d=ON \
    -D BUILD_opencv_features2d=ON \
    -D BUILD_opencv_flann=ON \
    -D BUILD_opencv_photo=ON \
    -D BUILD_opencv_video=ON \
    -D BUILD_opencv_highgui=OFF \
    -D BUILD_opencv_ml=ON \
    -D BUILD_opencv_stitching=ON \
    \
    `# ================================================================` \
    `# Useful Contrib Modules` \
    `# ================================================================` \
    -D BUILD_opencv_tracking=ON \
    -D BUILD_opencv_optflow=ON \
    -D BUILD_opencv_ximgproc=ON \
    -D BUILD_opencv_xobjdetect=ON \
    -D BUILD_opencv_bgsegm=ON \
    \
    `# ================================================================` \
    `# Exclude Unneeded Contrib Modules` \
    `# ================================================================` \
    -D BUILD_opencv_alphamat=OFF \
    -D BUILD_opencv_barcode=OFF \
    -D BUILD_opencv_cvv=OFF \
    -D BUILD_opencv_hdf=OFF \
    -D BUILD_opencv_viz=OFF \
    -D BUILD_opencv_rgbd=OFF \
    -D BUILD_opencv_sfm=OFF \
    -D BUILD_opencv_xfeatures2d=OFF \
    \
    `# ================================================================` \
    `# Disable GUI (Server Environment)` \
    `# ================================================================` \
    -D WITH_GTK=OFF \
    -D WITH_QT=OFF \
    -D WITH_OPENGL=OFF \
    -D WITH_VTK=OFF \
    \
    `# ================================================================` \
    `# Disable CUDA (CPU-only build)` \
    `# ================================================================` \
    -D WITH_CUDA=OFF \
    -D WITH_CUDNN=OFF \
    \
    `# ================================================================` \
    `# Disable Unneeded Features` \
    `# ================================================================` \
    -D BUILD_opencv_python2=OFF \
    -D BUILD_opencv_python3=OFF \
    -D BUILD_TESTS=OFF \
    -D BUILD_PERF_TESTS=OFF \
    -D BUILD_EXAMPLES=OFF \
    -D BUILD_DOCS=OFF \
    -D BUILD_opencv_apps=OFF \
    -D BUILD_JAVA=OFF \
    -D BUILD_opencv_java_bindings_generator=OFF \
    -D BUILD_opencv_js=OFF \
    -D BUILD_opencv_js_bindings_generator=OFF \
    \
    `# ================================================================` \
    `# Installation Configuration` \
    `# ================================================================` \
    -D INSTALL_C_EXAMPLES=OFF \
    -D INSTALL_PYTHON_EXAMPLES=OFF \
    -D OPENCV_GENERATE_PKGCONFIG=ON

# =============================================================================
# Build OpenCV
# =============================================================================
echo "Building OpenCV with $(nproc) cores..."
ninja -j$(nproc)

# =============================================================================
# Install OpenCV
# =============================================================================
echo "Installing OpenCV..."
ninja install

# =============================================================================
# Generate Build Info
# =============================================================================
echo "Generating build info..."
cat > opencv-build-info.txt << EOF
# OpenCV Build Information
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# =============================================================================

OpenCV Version: ${OPENCV_VERSION}
OpenVINO Version: ${OPENVINO_VERSION}
ISA Profile: ${ISA_PROFILE}
CPU Baseline: ${CPU_BASELINE}
CPU Dispatch: ${CPU_DISPATCH:-none}
AVX-512 Enabled: ${ENABLE_AVX512}
OpenVINO Enabled: ${WITH_OPENVINO}

# CMake Configuration Summary:
EOF

# Append CMake cache summary
grep -E "^(WITH_|BUILD_opencv_|CPU_|OpenCV_|OPENVINO)" CMakeCache.txt >> opencv-build-info.txt || true

echo ""
echo "=============================================="
echo "OpenCV Build Complete"
echo "=============================================="
echo "Installed to: /usr/local"
echo "Build info:   opencv-build-info.txt"
echo "=============================================="
