#!/bin/bash
# =============================================================================
# Print the recommended OpenCV build profile for this machine.
# =============================================================================

set -euo pipefail

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

if [ -z "${flags}" ]; then
    echo "unknown: /proc/cpuinfo flags not available"
    exit 1
fi

if [ "${has_avx512}" -eq 1 ]; then
    if [ "${has_vnni}" -eq 1 ]; then
        echo "tuned (AVX-512 VNNI detected)"
    else
        echo "tuned (AVX-512 detected)"
    fi
elif [ "${has_avx2}" -eq 1 ]; then
    echo "portable (AVX2 detected)"
else
    echo "portable (AVX2 not detected; build may not run)"
fi
