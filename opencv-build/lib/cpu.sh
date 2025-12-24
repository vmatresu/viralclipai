#!/bin/bash
# =============================================================================
# CPU Feature Detection Module
# =============================================================================
# Detects x86_64 SIMD capabilities for ISA profile selection.
#
# Usage:
#   source lib/cpu.sh
#   detect_cpu_features
#   profile=$(get_recommended_profile)
# =============================================================================

set -euo pipefail

# Global state (set by detect_cpu_features)
HAS_AVX2=0
HAS_AVX512=0
HAS_AVX512_VNNI=0
CPU_FLAGS=""

# Detect CPU features from system
detect_cpu_features() {
    HAS_AVX2=0
    HAS_AVX512=0
    HAS_AVX512_VNNI=0
    
    # Helper to check feature presence (Case Insensitive, Whole Word)
    _has_feature() {
        local feature="$1"
        
        # Method 1: lscpu (Preferred - handles arch abstraction better)
        if command -v lscpu >/dev/null 2>&1; then
            if lscpu | grep -iE "\b${feature}\b" >/dev/null 2>&1; then
                return 0
            fi
        fi
        
        # Method 2: /proc/cpuinfo (Fallback - standard Linux)
        # Matches lines starting with "flags" (x86) or "Features" (ARM)
        if grep -iE "^(flags|Features).*\b${feature}\b" /proc/cpuinfo >/dev/null 2>&1; then
            return 0
        fi
        
        return 1
    }
    
    # Check AVX2
    if _has_feature "avx2"; then
        HAS_AVX2=1
    fi
    
    # Check AVX-512 (Foundation + Byte/Word + Vector Length)
    # This combination ensures viable AVX-512 support for OpenCV
    if _has_feature "avx512f" && _has_feature "avx512bw" && _has_feature "avx512vl"; then
        HAS_AVX512=1
    fi
    
    # Check VNNI (Vector Neural Network Instructions)
    # Intel: avx512_vnni, AMD/Others might vary in reporting
    if _has_feature "avx512_vnni" || _has_feature "avx512vnni"; then
        HAS_AVX512_VNNI=1
    fi
    
    # Populate debug string for print_cpu_info (optional, for display only)
    if command -v lscpu >/dev/null 2>&1; then
         CPU_FLAGS="(from lscpu)"
    else
         CPU_FLAGS="(from /proc/cpuinfo)"
    fi
    
    return 0
}

# Get recommended ISA profile for this CPU
get_recommended_profile() {
    if [[ "${HAS_AVX512}" -eq 1 ]]; then
        echo "tuned"
    else
        echo "portable"
    fi
}

# Print CPU capabilities summary
print_cpu_info() {
    echo "CPU Feature Detection"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    if [[ -z "${CPU_FLAGS}" ]]; then
        echo "  Status: Unable to detect (/proc/cpuinfo unavailable)"
        return
    fi
    
    local avx2_status="❌ Not detected"
    local avx512_status="❌ Not detected"
    local vnni_status="❌ Not detected"
    
    [[ "${HAS_AVX2}" -eq 1 ]] && avx2_status="✅ Detected"
    [[ "${HAS_AVX512}" -eq 1 ]] && avx512_status="✅ Detected"
    [[ "${HAS_AVX512_VNNI}" -eq 1 ]] && vnni_status="✅ Detected"
    
    echo "  AVX2:        ${avx2_status}"
    echo "  AVX-512:     ${avx512_status}"
    echo "  AVX-512 VNNI: ${vnni_status}"
    echo ""
    echo "  Recommended profile: $(get_recommended_profile)"
    
    if [[ "${HAS_AVX2}" -eq 0 ]]; then
        echo ""
        echo "  ⚠️  WARNING: AVX2 not detected. OpenCV build may not run on this CPU."
    fi
}
