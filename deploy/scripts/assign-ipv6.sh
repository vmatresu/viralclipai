#!/bin/bash
# =============================================================================
# ViralClip AI - IPv6 Address Assignment Script
# =============================================================================
# Assigns multiple IPv6 addresses to Docker containers for IP rotation.
# This script is designed to be run after container startup.
#
# Usage:
#   sudo /usr/local/bin/assign-ipv6.sh
#   sudo /usr/local/bin/assign-ipv6.sh --container vclip-worker --count 1000 --start 100
#
# Environment Variables:
#   IPV6_SUBNET_PREFIX - IPv6 subnet prefix (e.g., 2001:41d0:xxx:xxxx::)
#   IPV6_CIDR_SUFFIX   - CIDR suffix (e.g., 64)
#
# Prerequisites:
#   - Docker container must be running
#   - Host must have IPv6 forwarding enabled
#   - ndppd must be configured and running
# =============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok()    { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Default configuration
CONTAINER_NAME="${CONTAINER_NAME:-vclip-worker}"
IP_COUNT="${IP_COUNT:-1000}"
START_SUFFIX="${START_SUFFIX:-100}"

# These should be set via environment or config file
SUBNET_PREFIX="${IPV6_SUBNET_PREFIX:-}"
CIDR_SUFFIX="${IPV6_CIDR_SUFFIX:-64}"

# Configuration file location (set by setup-server.sh)
CONFIG_FILE="/etc/viralclip/ipv6.conf"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --container) CONTAINER_NAME="$2"; shift 2 ;;
        --count) IP_COUNT="$2"; shift 2 ;;
        --start) START_SUFFIX="$2"; shift 2 ;;
        --prefix) SUBNET_PREFIX="$2"; shift 2 ;;
        --cidr) CIDR_SUFFIX="$2"; shift 2 ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --container NAME   Container name (default: vclip-worker)"
            echo "  --count N          Number of IPs to assign (default: 1000)"
            echo "  --start N          Starting suffix number (default: 100)"
            echo "  --prefix PREFIX    IPv6 subnet prefix (e.g., 2001:41d0:xxx::)"
            echo "  --cidr N           CIDR suffix (default: 64)"
            exit 0
            ;;
        *) log_error "Unknown argument: $1"; exit 1 ;;
    esac
done

# Load config file if exists
if [[ -f "$CONFIG_FILE" ]]; then
    log_info "Loading configuration from $CONFIG_FILE"
    source "$CONFIG_FILE"
fi

# Validate required configuration
if [[ -z "$SUBNET_PREFIX" ]]; then
    log_error "IPv6 subnet prefix not configured."
    log_error "Set IPV6_SUBNET_PREFIX environment variable or use --prefix flag."
    log_error "Example: --prefix '2001:41d0:xxx:xxxx::'"
    exit 1
fi

# Function to add IPs to a container
add_ips_to_container() {
    local container="$1"
    local start_suffix="$2"
    local count="$3"

    log_info "Waiting for container '$container' to be running..."

    # Wait for container to be running (up to 30 seconds)
    for i in {1..30}; do
        local pid
        pid=$(docker inspect -f '{{.State.Pid}}' "$container" 2>/dev/null) || true
        if [[ -n "$pid" ]] && [[ "$pid" != "0" ]]; then
            break
        fi
        sleep 1
    done

    # Get container PID
    local pid
    pid=$(docker inspect -f '{{.State.Pid}}' "$container" 2>/dev/null) || true

    if [[ -z "$pid" ]] || [[ "$pid" == "0" ]]; then
        log_error "Container '$container' is not running"
        return 1
    fi

    log_info "Container '$container' running with PID $pid"
    log_info "Assigning $count IPv6 addresses starting from ${SUBNET_PREFIX}$(printf '%x' "$start_suffix")"

    # Build batch command for efficiency (avoid spawning nsenter for each IP)
    local batch_cmd=""
    local assigned=0
    local skipped=0

    for i in $(seq 0 $((count - 1))); do
        local suffix=$((start_suffix + i))
        local hex_suffix
        hex_suffix=$(printf '%x' "$suffix")
        local ip="${SUBNET_PREFIX}${hex_suffix}"
        
        # Add to batch (suppress errors for already-assigned IPs)
        batch_cmd="${batch_cmd}ip -6 addr add ${ip}/${CIDR_SUFFIX} dev eth0 2>/dev/null && echo 'added' || echo 'skip';"
    done

    # Execute batch in the container's network namespace
    log_info "Executing batch IP assignment..."
    local result
    result=$(nsenter -t "$pid" -n -- bash -c "$batch_cmd" 2>&1) || true

    # Count results
    assigned=$(echo "$result" | grep -c "added" || true)
    skipped=$(echo "$result" | grep -c "skip" || true)

    log_ok "Assigned $assigned new IPv6 addresses ($skipped already existed)"
    return 0
}

# Main execution
log_info "=================================================="
log_info " ViralClip AI - IPv6 Address Assignment"
log_info "=================================================="
log_info "Container: $CONTAINER_NAME"
log_info "Subnet:    ${SUBNET_PREFIX}/${CIDR_SUFFIX}"
log_info "Count:     $IP_COUNT"
log_info "Start:     ${SUBNET_PREFIX}$(printf '%x' "$START_SUFFIX")"
log_info "=================================================="

add_ips_to_container "$CONTAINER_NAME" "$START_SUFFIX" "$IP_COUNT"

log_ok "IPv6 address assignment complete!"
log_info ""
log_info "Verify with: docker exec $CONTAINER_NAME ip -6 addr show eth0 | grep inet6 | wc -l"
