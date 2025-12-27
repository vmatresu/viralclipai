#!/bin/bash
# =============================================================================
# ViralClip AI - IPv6 Address Assignment Script
# =============================================================================
# Assigns multiple RANDOM IPv6 addresses to Docker containers for IP rotation.
# Random scattering across the /64 space provides better anti-detection
# compared to sequential addresses.
#
# Usage:
#   sudo ./assign-ipv6.sh                         # Assign to default container
#   sudo ./assign-ipv6.sh --container NAME        # Assign to specific container
#   sudo ./assign-ipv6.sh --count 10000           # Custom IP count
#   sudo ./assign-ipv6.sh verify                  # Verify current assignments
#   sudo ./assign-ipv6.sh list                    # List all assigned IPs
#
# Environment Variables:
#   IPV6_SUBNET_PREFIX - IPv6 subnet prefix (e.g., 2001:41d0:a:719c)
#   IPV6_CIDR_SUFFIX   - CIDR suffix (default: 64)
#
# Prerequisites:
#   - Docker container must be running
#   - Host must have IPv6 forwarding enabled
#   - ndppd must be configured and running
# =============================================================================

set -euo pipefail

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m'

log_info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok()    { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Default configuration
CONTAINER_NAME="${CONTAINER_NAME:-vclip-worker}"
IP_COUNT="${IP_COUNT:-10000}"
BATCH_SIZE="${BATCH_SIZE:-500}"

# These should be set via environment or config file
SUBNET_PREFIX="${IPV6_SUBNET_PREFIX:-}"
CIDR_SUFFIX="${IPV6_CIDR_SUFFIX:-64}"

# Configuration file location (set by setup-server.sh)
CONFIG_FILE="/etc/viralclip/ipv6.conf"

# Command to execute
COMMAND="assign"
OUTPUT_FORMAT="text"

# =============================================================================
# Argument Parsing
# =============================================================================
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --container) CONTAINER_NAME="$2"; shift 2 ;;
            --count) IP_COUNT="$2"; shift 2 ;;
            --prefix) SUBNET_PREFIX="$2"; shift 2 ;;
            --cidr) CIDR_SUFFIX="$2"; shift 2 ;;
            --batch-size) BATCH_SIZE="$2"; shift 2 ;;
            --json) OUTPUT_FORMAT="json"; shift ;;
            assign) COMMAND="assign"; shift ;;
            verify) COMMAND="verify"; shift ;;
            list) COMMAND="list"; shift ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *) log_error "Unknown argument: $1"; show_help; exit 1 ;;
        esac
    done
}

show_help() {
    cat <<EOF
ViralClip AI - IPv6 Address Assignment

Usage: $0 [OPTIONS] [COMMAND]

Commands:
  assign      Assign random IPv6 addresses (default)
  verify      Verify current IP assignments
  list        List all assigned IPv6 addresses

Options:
  --container NAME   Container name (default: vclip-worker)
  --count N          Number of IPs to assign (default: 10000)
  --prefix PREFIX    IPv6 subnet prefix (e.g., 2001:41d0:a:719c)
  --cidr N           CIDR suffix (default: 64)
  --batch-size N     IPs per batch (default: 500)
  --json             Output in JSON format (for verify command)
  -h, --help         Show this help message

Examples:
  # Assign 10,000 random IPs to default container
  sudo $0 assign

  # Assign 5,000 IPs to specific container
  sudo $0 --container my-container --count 5000 assign

  # Verify current assignments
  sudo $0 verify

  # List all IPs (warning: large output)
  sudo $0 list
EOF
}

# =============================================================================
# Container Functions
# =============================================================================
get_container_pid() {
    local container="$1"
    docker inspect -f '{{.State.Pid}}' "$container" 2>/dev/null || echo ""
}

wait_for_container() {
    local container="$1"
    local max_wait="${2:-30}"

    for i in $(seq 1 "$max_wait"); do
        local pid
        pid=$(get_container_pid "$container")

        if [[ -n "$pid" && "$pid" != "0" ]]; then
            return 0
        fi

        sleep 1
    done

    return 1
}

container_exists() {
    local container="$1"
    docker inspect "$container" &>/dev/null
}

# =============================================================================
# IP Assignment Functions
# =============================================================================

# Assign random IPv6 addresses scattered across the /64 space
# This provides better anti-detection than sequential addresses
assign_random_ips() {
    local container="$1"
    local count="$2"
    local subnet_prefix="$3"
    local cidr="${4:-64}"

    # Check container exists
    if ! container_exists "$container"; then
        log_error "Container $container does not exist"
        return 1
    fi

    # Wait for container to be ready
    log_info "Waiting for container '$container' to be running..."
    if ! wait_for_container "$container" 30; then
        log_error "Container $container not running after 30s"
        return 1
    fi

    local pid
    pid=$(get_container_pid "$container")

    if [[ -z "$pid" || "$pid" == "0" ]]; then
        log_error "Container $container has no valid PID"
        return 1
    fi

    log_info "Container '$container' running with PID $pid"
    log_info "Assigning $count RANDOM IPv6 addresses from ${subnet_prefix}::/64"
    log_info "Batch size: $BATCH_SIZE"
    echo ""

    # Build batch command for efficiency
    local batch_cmd=""
    local assigned=0
    local errors=0

    for i in $(seq 1 "$count"); do
        # Generate random 4 hex groups (64 bits for the host portion)
        local r1 r2 r3 r4
        r1=$(printf '%x' $((RANDOM % 65536)))
        r2=$(printf '%x' $((RANDOM % 65536)))
        r3=$(printf '%x' $((RANDOM % 65536)))
        r4=$(printf '%x' $((RANDOM % 65536)))

        local ip="${subnet_prefix}:${r1}:${r2}:${r3}:${r4}"
        batch_cmd="${batch_cmd}ip -6 addr add ${ip}/${cidr} dev eth0 2>/dev/null || true;"

        # Execute in batches to avoid command line length limits and show progress
        if [[ $((i % BATCH_SIZE)) -eq 0 ]]; then
            if nsenter -t "$pid" -n -- bash -c "$batch_cmd" 2>/dev/null; then
                assigned=$((assigned + BATCH_SIZE))
            else
                errors=$((errors + 1))
            fi
            batch_cmd=""
            log_info "  Progress: $i/$count IPs..."
        fi
    done

    # Execute remaining commands
    if [[ -n "$batch_cmd" ]]; then
        local remaining=$((count % BATCH_SIZE))
        [[ "$remaining" -eq 0 ]] && remaining=$BATCH_SIZE
        if nsenter -t "$pid" -n -- bash -c "$batch_cmd" 2>/dev/null; then
            assigned=$((assigned + remaining))
        else
            errors=$((errors + 1))
        fi
    fi

    echo ""
    if [[ "$errors" -eq 0 ]]; then
        log_ok "Assigned $assigned random addresses successfully"
        return 0
    else
        log_warn "Completed with $errors batch errors (some IPs may have collided)"
        return 0
    fi
}

verify_container_ips() {
    local container="$1"

    if ! container_exists "$container"; then
        if [[ "$OUTPUT_FORMAT" == "json" ]]; then
            echo "{\"error\": \"Container not found\", \"container\": \"$container\"}"
        else
            log_error "Container $container not found"
        fi
        return 1
    fi

    local pid
    pid=$(get_container_pid "$container")

    if [[ -z "$pid" || "$pid" == "0" ]]; then
        if [[ "$OUTPUT_FORMAT" == "json" ]]; then
            echo "{\"error\": \"Container not running\", \"container\": \"$container\"}"
        else
            log_error "Container $container not running"
        fi
        return 1
    fi

    # Count IPv6 addresses on eth0
    local ipv6_count
    ipv6_count=$(nsenter -t "$pid" -n -- ip -6 addr show dev eth0 scope global 2>/dev/null | grep -c inet6 || echo "0")

    if [[ "$OUTPUT_FORMAT" == "json" ]]; then
        echo "{\"container\": \"$container\", \"ipv6_count\": $ipv6_count, \"expected\": $IP_COUNT}"
    else
        log_info "Container: $container"
        log_info "IPv6 addresses: $ipv6_count"
        log_info "Expected: $IP_COUNT"

        if [[ "$ipv6_count" -ge "$IP_COUNT" ]]; then
            log_ok "Container has sufficient IPv6 addresses"
        else
            log_warn "Container has fewer IPs than expected"
        fi
    fi

    return 0
}

list_container_ips() {
    local container="$1"

    if ! container_exists "$container"; then
        log_error "Container $container not found"
        return 1
    fi

    local pid
    pid=$(get_container_pid "$container")

    if [[ -z "$pid" || "$pid" == "0" ]]; then
        log_error "Container $container not running"
        return 1
    fi

    log_info "IPv6 addresses for $container (global scope only):"
    echo ""
    nsenter -t "$pid" -n -- ip -6 addr show dev eth0 scope global 2>/dev/null | grep inet6 | awk '{print "  " $2}'
}

# =============================================================================
# Main Execution
# =============================================================================
main() {
    parse_args "$@"

    # Load config file if exists
    if [[ -f "$CONFIG_FILE" ]]; then
        log_info "Loading configuration from $CONFIG_FILE"
        # shellcheck source=/dev/null
        source "$CONFIG_FILE"
        # Config file uses different variable names
        SUBNET_PREFIX="${IPV6_SUBNET_PREFIX:-$SUBNET_PREFIX}"
        CIDR_SUFFIX="${IPV6_CIDR_SUFFIX:-$CIDR_SUFFIX}"
    fi

    case "$COMMAND" in
        assign)
            # Validate required configuration for assign
            if [[ -z "$SUBNET_PREFIX" ]]; then
                log_error "IPv6 subnet prefix not configured."
                log_error "Set IPV6_SUBNET_PREFIX environment variable or use --prefix flag."
                log_error "Example: --prefix '2001:41d0:a:719c'"
                exit 1
            fi

            log_info "=================================================="
            log_info " ViralClip AI - IPv6 Address Assignment"
            log_info "=================================================="
            log_info "Container: $CONTAINER_NAME"
            log_info "Subnet:    ${SUBNET_PREFIX}::/${CIDR_SUFFIX}"
            log_info "Count:     $IP_COUNT (random)"
            log_info "=================================================="
            echo ""

            assign_random_ips "$CONTAINER_NAME" "$IP_COUNT" "$SUBNET_PREFIX" "$CIDR_SUFFIX"

            echo ""
            log_ok "IPv6 address assignment complete!"
            log_info ""
            log_info "Verify with: $0 verify"
            log_info "Or manually: docker exec $CONTAINER_NAME ip -6 addr show eth0 | grep inet6 | wc -l"
            ;;

        verify)
            verify_container_ips "$CONTAINER_NAME"
            ;;

        list)
            list_container_ips "$CONTAINER_NAME"
            ;;

        *)
            log_error "Unknown command: $COMMAND"
            show_help
            exit 1
            ;;
    esac
}

# Allow sourcing without execution
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
