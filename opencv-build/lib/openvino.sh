#!/bin/bash
# =============================================================================
# OpenVINO Auto-Installation Module
# =============================================================================
# Auto-detects Ubuntu version and installs OpenVINO from Intel APT repository.
#
# Usage:
#   source lib/openvino.sh
#   check_openvino_installed && echo "Already installed"
#   install_openvino "2024.4"
# =============================================================================

set -euo pipefail

# Check if OpenVINO is installed
check_openvino_installed() {
    # Check common config file locations
    local search_dirs=(
        "/opt/intel/openvino/runtime/cmake"
        "/opt/intel/openvino_*/runtime/cmake"
        "/usr/lib/x86_64-linux-gnu/cmake/openvino*"
        "/usr/share/openvino/cmake"
        "/usr/local/lib/cmake/openvino"
    )
    
    for pattern in "${search_dirs[@]}"; do
        for dir in ${pattern}; do
            if [[ -f "${dir}/OpenVINOConfig.cmake" ]] 2>/dev/null; then
                return 0
            fi
        done
    done
    
    # Fallback: check dpkg
    if dpkg -l 2>/dev/null | grep -qi "openvino"; then
        return 0
    fi
    
    return 1
}

# Find OpenVINO CMake directory
find_openvino_cmake_dir() {
    local search_paths=(
        "/opt/intel/openvino/runtime/cmake"
        "/opt/intel/openvino_2024.4/runtime/cmake"
        "/opt/intel/openvino_2024.4.0/runtime/cmake"
        "/usr/lib/x86_64-linux-gnu/cmake/openvino"
        "/usr/share/openvino/cmake"
        "/usr/local/lib/cmake/openvino"
    )
    
    for dir in "${search_paths[@]}"; do
        if [[ -f "${dir}/OpenVINOConfig.cmake" ]]; then
            echo "${dir}"
            return 0
        fi
    done
    
    # Fallback: filesystem search
    local found
    found="$(find /opt/intel /usr /usr/local -type f -name 'OpenVINOConfig.cmake' 2>/dev/null | head -n1 || true)"
    if [[ -n "${found}" ]]; then
        dirname "${found}"
        return 0
    fi
    
    return 1
}

# Source OpenVINO environment if available
source_openvino_env() {
    local version="${1:-2024.4}"
    local setupvars_paths=(
        "/opt/intel/openvino/setupvars.sh"
        "/opt/intel/openvino_${version}/setupvars.sh"
        "/opt/intel/openvino_${version}.0/setupvars.sh"
        "/usr/share/openvino/setupvars.sh"
    )
    
    for path in "${setupvars_paths[@]}"; do
        if [[ -f "${path}" ]]; then
            echo "Sourcing ${path}"
            # shellcheck disable=SC1090
            source "${path}"
            return 0
        fi
    done
    
    echo "Note: No setupvars.sh found (CMake should still work)"
    return 0
}

# Install OpenVINO (auto-detect Ubuntu version)
install_openvino() {
    local openvino_version="${1:-2024.4}"
    
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "OpenVINO Auto-Installation"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    # Detect Ubuntu version
    if [[ ! -f /etc/os-release ]]; then
        echo "ERROR: Cannot detect OS version. /etc/os-release not found."
        return 1
    fi
    
    # shellcheck disable=SC1091
    . /etc/os-release
    local ubuntu_version="${VERSION_ID}"
    
    echo "Detected OS: ${NAME} ${VERSION_ID}"
    
    # Map Ubuntu version to Intel repo codename
    local intel_repo_codename
    case "${ubuntu_version}" in
        24.04) intel_repo_codename="ubuntu24" ;;
        22.04) intel_repo_codename="ubuntu22" ;;
        20.04) intel_repo_codename="ubuntu20" ;;
        *)
            echo "WARNING: Ubuntu ${ubuntu_version} not officially supported."
            echo "Attempting ubuntu24 repository..."
            intel_repo_codename="ubuntu24"
            ;;
    esac
    
    echo "Using Intel repository: ${intel_repo_codename}"
    echo ""
    
    # Install prerequisites (use sudo only if not root, e.g., in Docker as root we don't have sudo installed)
    if [[ $EUID -eq 0 ]]; then
        SUDO=""
    else
        SUDO="sudo"
    fi
    echo "Installing prerequisites..."
    ${SUDO} apt-get update
    ${SUDO} apt-get install -y gnupg ca-certificates wget
    
    # Add Intel GPG key and repository only if not already present
    if [[ ! -f /etc/apt/sources.list.d/intel-openvino-2024.list ]]; then
        echo "Adding Intel GPG key..."
        wget -qO- https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB | \
            ${SUDO} gpg --dearmor -o /usr/share/keyrings/intel-openvino.gpg 2>/dev/null || true
        
        echo "Adding OpenVINO repository..."
        echo "deb [signed-by=/usr/share/keyrings/intel-openvino.gpg] https://apt.repos.intel.com/openvino/2024 ${intel_repo_codename} main" | \
            ${SUDO} tee /etc/apt/sources.list.d/intel-openvino-2024.list > /dev/null
        
        echo "Updating package list..."
        ${SUDO} apt-get update
    else
        echo "OpenVINO repository already configured"
    fi
    
    # Find available package
    echo "Searching for OpenVINO packages..."
    local openvino_pkg=""
    local candidate_packages=(
        "openvino-${openvino_version}.0"
        "openvino-${openvino_version}"
        "openvino"
    )
    
    for pkg in "${candidate_packages[@]}"; do
        if apt-cache show "${pkg}" &>/dev/null; then
            openvino_pkg="${pkg}"
            echo "Found package: ${pkg}"
            break
        fi
    done
    
    if [[ -z "${openvino_pkg}" ]]; then
        echo "ERROR: No OpenVINO package found in repository."
        echo "Available packages:"
        apt-cache search openvino | head -20 || echo "  (none)"
        return 1
    fi
    
    # Install OpenVINO
    echo "Installing ${openvino_pkg}..."
${SUDO} apt-get install -y "${openvino_pkg}"
    
    # Install dev package if available
    local dev_pkg="${openvino_pkg}-dev"
    if apt-cache show "${dev_pkg}" &>/dev/null; then
        echo "Installing ${dev_pkg}..."
        ${SUDO} apt-get install -y "${dev_pkg}"
    fi
    
    echo ""
    echo "✅ OpenVINO installation complete!"
    return 0
}
