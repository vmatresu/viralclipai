#!/bin/bash
# =============================================================================
# Auto-select OpenCV build profile based on local CPU capabilities.
# - portable: AVX2 baseline (safe everywhere)
# - tuned: AVX-512 dispatch (fastest, requires AVX-512)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${1:-auto}"

flags="$(awk -F: '/^(flags|Features)/ {print $2; exit}' /proc/cpuinfo 2>/dev/null || true)"

has_flag() {
    case " ${flags} " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

has_avx2=0
has_avx512=0
has_vnni=0

if has_flag avx2; then
    has_avx2=1
fi

if has_flag avx512f && has_flag avx512bw && has_flag avx512vl; then
    has_avx512=1
fi

if has_flag avx512vnni; then
    has_vnni=1
fi

log_capabilities() {
    if [ -z "${flags}" ]; then
        echo "Detected CPU flags: unavailable (/proc/cpuinfo missing)."
        return
    fi
    echo "Detected CPU flags: avx2=${has_avx2} avx512=${has_avx512} avx512vnni=${has_vnni}"
}

build_portable() {
    "${SCRIPT_DIR}/build-portable.sh"
}

build_tuned() {
    if [ "${has_avx512}" -ne 1 ]; then
        echo "ERROR: AVX-512 not detected. Tuned build requires AVX-512F/BW/VL."
        exit 1
    fi
    "${SCRIPT_DIR}/build-tuned.sh"
}

case "${MODE}" in
    auto)
        log_capabilities
        if [ "${has_avx512}" -eq 1 ]; then
            echo "Auto-select: tuned (AVX-512 detected)"
            build_tuned
        else
            if [ "${has_avx2}" -eq 1 ]; then
                echo "Auto-select: portable (AVX-512 not detected)"
            else
                echo "Auto-select: portable (AVX2 not detected; build may not run on this CPU)"
            fi
            build_portable
        fi
        ;;
    portable)
        log_capabilities
        build_portable
        ;;
    tuned)
        log_capabilities
        build_tuned
        ;;
    both)
        log_capabilities
        if [ "${has_avx2}" -ne 1 ]; then
            echo "WARNING: AVX2 not detected; portable build may not run on this CPU."
        fi
        build_portable
        if [ "${has_avx512}" -eq 1 ]; then
            build_tuned
        else
            echo "Skipping tuned build: AVX-512 not detected."
        fi
        ;;
    *)
        echo "Usage: $0 [auto|portable|tuned|both]"
        exit 1
        ;;
esac
