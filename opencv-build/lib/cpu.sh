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

# Detect CPU features from /proc/cpuinfo
detect_cpu_features() {
    CPU_FLAGS="$(awk -F: '/^(flags|Features)/ {print $2; exit}' /proc/cpuinfo 2>/dev/null || true)"
    
    if [[ -z "${CPU_FLAGS}" ]]; then
        echo "WARNING: Cannot read CPU flags from /proc/cpuinfo" >&2
        return 1
    fi
    
    _has_flag() {
        case " ${CPU_FLAGS} " in
            *" $1 "*) return 0 ;;
            *) return 1 ;;
        esac
    }
    
    if _has_flag avx2; then
        HAS_AVX2=1
    fi
    
    if _has_flag avx512f && _has_flag avx512bw && _has_flag avx512vl; then
        HAS_AVX512=1
    fi
    
    if _has_flag avx512vnni; then
        HAS_AVX512_VNNI=1
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
