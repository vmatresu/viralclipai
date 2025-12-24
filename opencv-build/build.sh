#!/bin/bash
# =============================================================================
# OpenCV Build System - Unified Entry Point
# =============================================================================
# Single entry point for building OpenCV with OpenVINO integration.
#
# Usage:
#   ./build.sh                     # Auto-detect CPU, build with Docker
#   ./build.sh --native            # Build directly on host (no Docker)
#   ./build.sh --profile portable  # Force portable profile
#   ./build.sh --profile tuned     # Force tuned profile (requires AVX-512)
#   ./build.sh info                # Show CPU capabilities
#   ./build.sh install-openvino    # Install OpenVINO only
#   ./build.sh --help              # Show help
#
# Environment:
#   OPENCV_VERSION     - OpenCV version (default: 4.12.0)
#   OPENVINO_VERSION   - OpenVINO version (default: 2024.4)
# =============================================================================

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly OPENCV_VERSION="${OPENCV_VERSION:-4.12.0}"
readonly OPENVINO_VERSION="${OPENVINO_VERSION:-2024.4}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# =============================================================================
# Load Modules
# =============================================================================
# shellcheck source=lib/cpu.sh
source "${SCRIPT_DIR}/lib/cpu.sh"
# shellcheck source=lib/openvino.sh
source "${SCRIPT_DIR}/lib/openvino.sh"
# shellcheck source=lib/cmake-config.sh
source "${SCRIPT_DIR}/lib/cmake-config.sh"

# =============================================================================
# Helper Functions
# =============================================================================
log_info() { echo -e "${BLUE}ℹ${NC} $*"; }
log_success() { echo -e "${GREEN}✅${NC} $*"; }
log_warning() { echo -e "${YELLOW}⚠️${NC} $*"; }
log_error() { echo -e "${RED}❌${NC} $*" >&2; }

print_header() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $*${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_config() {
    print_header "OpenCV Build Configuration"
    echo "  OpenCV Version:   ${OPENCV_VERSION}"
    echo "  OpenVINO Version: ${OPENVINO_VERSION}"
    echo "  Profile:          ${PROFILE}"
    echo "  Mode:             ${BUILD_MODE}"
    echo ""
}

show_help() {
    cat <<EOF
OpenCV Build System v1.0

USAGE:
    ./build.sh [OPTIONS] [COMMAND]

COMMANDS:
    (default)          Build OpenCV (auto-detect profile, Docker mode)
    info               Show CPU capabilities and recommended profile
    install-openvino   Install OpenVINO only (no OpenCV build)
    help               Show this help message

OPTIONS:
    --profile <name>   ISA profile: 'portable' (AVX2) or 'tuned' (AVX-512)
                       Default: auto-detect based on CPU
    --native           Build directly on host (no Docker)
                       Use this when running inside a container or VM
    -h, --help         Show this help message

EXAMPLES:
    ./build.sh                      # Auto-detect, Docker build
    ./build.sh --native             # Build on host, auto-detect profile
    ./build.sh --profile portable   # Force portable, Docker build
    ./build.sh info                 # Just show CPU info

ENVIRONMENT:
    OPENCV_VERSION     Override OpenCV version (default: ${OPENCV_VERSION})
    OPENVINO_VERSION   Override OpenVINO version (default: ${OPENVINO_VERSION})

EOF
}

# =============================================================================
# Commands
# =============================================================================

cmd_info() {
    print_header "System Information"
    detect_cpu_features || true
    print_cpu_info
    echo ""
    
    print_header "OpenVINO Status"
    if check_openvino_installed; then
        log_success "OpenVINO is installed"
        local cmake_dir
        if cmake_dir=$(find_openvino_cmake_dir); then
            echo "  CMake dir: ${cmake_dir}"
        fi
    else
        log_warning "OpenVINO is not installed"
        echo "  Run: ./build.sh install-openvino"
    fi
    echo ""
    
    print_header "Build Configuration"
    echo "  OpenCV Version:   ${OPENCV_VERSION}"
    echo "  OpenVINO Version: ${OPENVINO_VERSION}"
}

cmd_install_openvino() {
    print_header "Installing OpenVINO ${OPENVINO_VERSION}"
    
    if check_openvino_installed; then
        log_success "OpenVINO is already installed"
        local cmake_dir
        if cmake_dir=$(find_openvino_cmake_dir); then
            echo "  CMake dir: ${cmake_dir}"
        fi
        return 0
    fi
    
    install_openvino "${OPENVINO_VERSION}"
}

cmd_build_docker() {
    local profile="${1}"
    
    print_header "Building OpenCV with Docker"
    echo "  Profile: ${profile}"
    echo ""
    
    # Validate tuned profile has AVX-512
    if [[ "${profile}" == "tuned" ]] && [[ "${HAS_AVX512:-0}" -ne 1 ]]; then
        log_warning "Tuned profile requires AVX-512, but it was not detected"
        log_warning "Proceeding anyway since it was explicitly requested..."
    fi
    DOCKER_BUILDKIT=1 BUILDKIT_PROGRESS=plain docker build \
        --target export \
        --build-arg ISA_PROFILE="${profile}" \
        --build-arg OPENCV_VERSION="${OPENCV_VERSION}" \
        --build-arg OPENVINO_VERSION="${OPENVINO_VERSION}" \
        -f "${SCRIPT_DIR}/Dockerfile.openvino" \
        -o type=local,dest="${SCRIPT_DIR}/artifacts" \
        "${SCRIPT_DIR}"
    
    echo ""
    log_success "Build complete! Artifacts in: ${SCRIPT_DIR}/artifacts/"
    ls -la "${SCRIPT_DIR}/artifacts/"
}

cmd_build_native() {
    local profile="${1}"
    
    print_header "Building OpenCV Natively"
    echo "  Profile: ${profile}"
    echo ""
    
    # Validate tuned profile has AVX-512
    if [[ "${profile}" == "tuned" ]] && [[ "${HAS_AVX512:-0}" -ne 1 ]]; then
        log_warning "Tuned profile requires AVX-512, but it was not detected"
        log_warning "Proceeding anyway since it was explicitly requested..."
    fi
    
    # Step 1: Ensure OpenVINO is installed
    print_header "Step 1/4: Checking OpenVINO"
    if ! check_openvino_installed; then
        log_warning "OpenVINO not found. Installing..."
        install_openvino "${OPENVINO_VERSION}"
    else
        log_success "OpenVINO is installed"
    fi
    
    # Source OpenVINO environment
    source_openvino_env "${OPENVINO_VERSION}"
    
    # Find OpenVINO CMake directory
    local openvino_cmake_dir
    if ! openvino_cmake_dir=$(find_openvino_cmake_dir); then
        log_error "OpenVINO CMake configuration not found"
        log_info "Try running: ./build.sh install-openvino"
        exit 1
    fi
    echo "  OpenVINO CMake dir: ${openvino_cmake_dir}"
    
    # Step 2: Clone OpenCV source (if not present)
    print_header "Step 2/4: Preparing OpenCV Source"
    
    # Detect source directories - support running from project root or build/ subdir
    local opencv_src=""
    local opencv_contrib=""
    local build_dir=""
    
    if [[ -d "${SCRIPT_DIR}/opencv" ]]; then
        # Running from project root (normal case)
        opencv_src="${SCRIPT_DIR}/opencv"
        opencv_contrib="${SCRIPT_DIR}/opencv_contrib"
        build_dir="${SCRIPT_DIR}/build"
    elif [[ -d "${SCRIPT_DIR}/../opencv" ]]; then
        # Running from build/ subdirectory (Docker case)
        opencv_src="${SCRIPT_DIR}/../opencv"
        opencv_contrib="${SCRIPT_DIR}/../opencv_contrib"
        build_dir="${SCRIPT_DIR}"
    else
        # No source found, will clone to project root
        opencv_src="${SCRIPT_DIR}/opencv"
        opencv_contrib="${SCRIPT_DIR}/opencv_contrib"
        build_dir="${SCRIPT_DIR}/build"
    fi
    
    if [[ ! -d "${opencv_src}" ]]; then
        log_info "Cloning OpenCV ${OPENCV_VERSION}..."
        git clone --depth 1 -b "${OPENCV_VERSION}" \
            https://github.com/opencv/opencv.git "${opencv_src}"
    else
        log_success "OpenCV source already present: ${opencv_src}"
    fi
    
    if [[ ! -d "${opencv_contrib}" ]]; then
        log_info "Cloning OpenCV contrib ${OPENCV_VERSION}..."
        git clone --depth 1 -b "${OPENCV_VERSION}" \
            https://github.com/opencv/opencv_contrib.git "${opencv_contrib}"
    else
        log_success "OpenCV contrib already present: ${opencv_contrib}"
    fi
    mkdir -p "${build_dir}"
    cd "${build_dir}"
    
    # Step 3: Configure with CMake
    print_header "Step 3/4: Configuring with CMake"
    run_cmake_configure "${profile}" "${openvino_cmake_dir}" "${OPENCV_VERSION}" "${opencv_src}" "${opencv_contrib}"
    
    # Verify OpenVINO integration
    verify_openvino_cmake
    
    # Step 4: Build and install
    print_header "Step 4/4: Building with Ninja"
    local num_cores
    num_cores=$(nproc)
    log_info "Building with ${num_cores} cores..."
    ninja -j"${num_cores}"
    
    log_info "Installing..."
    if command -v sudo >/dev/null 2>&1; then
        sudo ninja install
    else
        ninja install
    fi
    
    # Generate build info
    generate_build_info "${profile}" "${OPENCV_VERSION}" "${OPENVINO_VERSION}"
    
    # Verify linking
    verify_openvino_linking || log_warning "OpenVINO linking verification failed"
    
    echo ""
    log_success "OpenCV build complete!"
    echo "  Installed to: /usr/local"
    echo "  Build info:   ${build_dir}/opencv-build-info.txt"
}

# =============================================================================
# Main
# =============================================================================

main() {
    # Parse arguments
    local COMMAND=""
    local PROFILE=""
    local BUILD_MODE="docker"
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            info)
                COMMAND="info"
                shift
                ;;
            install-openvino)
                COMMAND="install-openvino"
                shift
                ;;
            help|--help|-h)
                show_help
                exit 0
                ;;
            --profile)
                PROFILE="${2:-}"
                if [[ -z "${PROFILE}" ]]; then
                    log_error "--profile requires a value (portable or tuned)"
                    exit 1
                fi
                if [[ "${PROFILE}" != "portable" && "${PROFILE}" != "tuned" ]]; then
                    log_error "Invalid profile: ${PROFILE}. Use 'portable' or 'tuned'"
                    exit 1
                fi
                shift 2
                ;;
            --native)
                BUILD_MODE="native"
                shift
                ;;
            *)
                log_error "Unknown argument: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # Default command is build
    COMMAND="${COMMAND:-build}"
    
    # Auto-detect profile if not specified
    if [[ -z "${PROFILE}" ]]; then
        detect_cpu_features || true
        PROFILE=$(get_recommended_profile)
        log_info "Auto-detected profile: ${PROFILE}"
    fi
    
    # Execute command
    case "${COMMAND}" in
        info)
            cmd_info
            ;;
        install-openvino)
            cmd_install_openvino
            ;;
        build)
            print_config
            if [[ "${BUILD_MODE}" == "native" ]]; then
                cmd_build_native "${PROFILE}"
            else
                cmd_build_docker "${PROFILE}"
            fi
            ;;
    esac
}

main "$@"
