#!/bin/bash
# =============================================================================
# OpenCV CMake Configuration Generator
# =============================================================================
# Generates CMake arguments for OpenCV build with OpenVINO integration.
#
# Usage:
#   source lib/cmake-config.sh
#   run_cmake_configure "portable" "/path/to/openvino/cmake"
# =============================================================================

set -euo pipefail

# Generate and run CMake configuration
run_cmake_configure() {
    local isa_profile="${1:-portable}"
    local openvino_cmake_dir="${2:-}"
    local opencv_version="${3:-4.12.0}"
    local opencv_src="${4:-../opencv}"
    local opencv_contrib="${5:-../opencv_contrib}"
    
    # ISA Profile Configuration
    local cpu_baseline="AVX2"
    local cpu_dispatch=""
    local enable_avx512="OFF"
    
    case "${isa_profile}" in
        portable)
            cpu_baseline="AVX2"
            cpu_dispatch=""
            enable_avx512="OFF"
            echo "Using PORTABLE profile: AVX2 baseline, no AVX-512"
            ;;
        tuned)
            cpu_baseline="AVX2"
            cpu_dispatch="AVX512_SKX;AVX512_ICL"
            enable_avx512="ON"
            echo "Using TUNED profile: AVX2 baseline with AVX-512 dispatch"
            ;;
        *)
            echo "ERROR: Unknown ISA profile: ${isa_profile}"
            return 1
            ;;
    esac
    
    # Build OpenVINO flag
    local with_openvino="OFF"
    local openvino_dir_arg=""
    if [[ -n "${openvino_cmake_dir}" ]]; then
        with_openvino="ON"
        openvino_dir_arg="-D OpenVINO_DIR=${openvino_cmake_dir}"
    fi
    
    # Run CMake
    cmake -G Ninja "${opencv_src}" \
        -D CMAKE_BUILD_TYPE=Release \
        -D CMAKE_INSTALL_PREFIX=/usr/local \
        -D OPENCV_EXTRA_MODULES_PATH="${opencv_contrib}/modules" \
        \
        -D WITH_OPENVINO=${with_openvino} \
        ${openvino_dir_arg} \
        \
        -D CPU_BASELINE=${cpu_baseline} \
        -D CPU_DISPATCH="${cpu_dispatch}" \
        -D ENABLE_AVX2=ON \
        -D ENABLE_AVX512=${enable_avx512} \
        \
        -D WITH_TBB=ON \
        -D WITH_OPENMP=OFF \
        -D WITH_PTHREADS_PF=ON \
        \
        -D WITH_IPP=ON \
        -D BUILD_IPP_IW=ON \
        \
        -D WITH_FFMPEG=ON \
        \
        -D WITH_PNG=ON \
        -D WITH_TIFF=ON \
        -D WITH_WEBP=ON \
        -D WITH_JPEG=ON \
        \
        -D BUILD_opencv_dnn=ON \
        -D BUILD_opencv_objdetect=ON \
        -D BUILD_opencv_face=ON \
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
        -D BUILD_opencv_tracking=ON \
        -D BUILD_opencv_optflow=ON \
        -D BUILD_opencv_ximgproc=ON \
        -D BUILD_opencv_xobjdetect=ON \
        -D BUILD_opencv_bgsegm=ON \
        \
        -D BUILD_opencv_alphamat=OFF \
        -D BUILD_opencv_barcode=OFF \
        -D BUILD_opencv_cvv=OFF \
        -D BUILD_opencv_hdf=OFF \
        -D BUILD_opencv_viz=OFF \
        -D BUILD_opencv_rgbd=OFF \
        -D BUILD_opencv_sfm=OFF \
        -D BUILD_opencv_xfeatures2d=OFF \
        \
        -D WITH_GTK=OFF \
        -D WITH_QT=OFF \
        -D WITH_OPENGL=OFF \
        -D WITH_VTK=OFF \
        \
        -D WITH_CUDA=OFF \
        -D WITH_CUDNN=OFF \
        \
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
        -D INSTALL_C_EXAMPLES=OFF \
        -D INSTALL_PYTHON_EXAMPLES=OFF \
        -D OPENCV_GENERATE_PKGCONFIG=ON
}

# Verify OpenVINO integration in CMake cache
verify_openvino_cmake() {
    local openvino_enabled
    openvino_enabled=$(grep "^WITH_OPENVINO:BOOL=" CMakeCache.txt 2>/dev/null | cut -d= -f2 || echo "")
    
    if [[ "${openvino_enabled}" != "ON" ]]; then
        echo "ERROR: OpenVINO integration failed (WITH_OPENVINO=${openvino_enabled})"
        echo "Debug info:"
        grep -iE "openvino" CMakeCache.txt 2>/dev/null | head -10 || echo "  (no matches)"
        return 1
    fi
    
    echo "✅ OpenVINO integration verified"
    return 0
}

# Verify OpenVINO linking after build
verify_openvino_linking() {
    local dnn_lib="/usr/local/lib/libopencv_dnn.so"
    
    if [[ ! -f "${dnn_lib}" ]]; then
        echo "WARNING: ${dnn_lib} not found after install"
        return 1
    fi
    
    if ldd "${dnn_lib}" | grep -q "libopenvino"; then
        echo "✅ libopencv_dnn.so links to libopenvino"
        ldd "${dnn_lib}" | grep openvino
        return 0
    else
        echo "ERROR: libopencv_dnn.so does NOT link to libopenvino"
        return 1
    fi
}

# Generate build info file
generate_build_info() {
    local isa_profile="${1:-portable}"
    local opencv_version="${2:-4.12.0}"
    local openvino_version="${3:-2024.4}"
    
    local cpu_baseline="AVX2"
    local cpu_dispatch=""
    local enable_avx512="OFF"
    local with_openvino="ON"
    
    if [[ "${isa_profile}" == "tuned" ]]; then
        cpu_dispatch="AVX512_SKX;AVX512_ICL"
        enable_avx512="ON"
    fi
    
    cat > opencv-build-info.txt <<EOF
# OpenCV Build Information
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# =============================================================================

OpenCV Version: ${opencv_version}
OpenVINO Version: ${openvino_version}
ISA Profile: ${isa_profile}
CPU Baseline: ${cpu_baseline}
CPU Dispatch: ${cpu_dispatch:-none}
AVX-512 Enabled: ${enable_avx512}
OpenVINO Enabled: ${with_openvino}

# CMake Configuration Summary:
EOF

    grep -E "^(WITH_|BUILD_opencv_|CPU_|OpenCV_|OPENVINO)" CMakeCache.txt >> opencv-build-info.txt 2>/dev/null || true
}
