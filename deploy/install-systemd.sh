#!/usr/bin/env bash
# =============================================================================
# ViralClip AI - Unified Systemd Service Installer
# =============================================================================
# Installs, uninstalls, or updates systemd services for API/Worker containers.
#
# Usage:
#   sudo ./install-systemd.sh api              # Install API service
#   sudo ./install-systemd.sh worker           # Install Worker service
#   sudo ./install-systemd.sh api uninstall    # Remove API service
#   sudo ./install-systemd.sh worker uninstall # Remove Worker service
#   sudo ./install-systemd.sh api status       # Check API service status
#
# Requirements:
#   - Ubuntu 20.04+ / Debian 11+
#   - Docker and Docker Compose v2 installed
#   - Repository cloned to /var/www/viralclipai-backend
#   - .env file configured
# =============================================================================

set -euo pipefail

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APP_DIR="/var/www/viralclipai-backend"
readonly SYSTEMD_DIR="/etc/systemd/system"
readonly DEPLOY_USER="deploy"
readonly DEPLOY_GROUP="deploy"

# Service definitions (name -> compose file)
declare -A SERVICES=(
    ["api"]="docker-compose.api.yml"
    ["worker"]="docker-compose.worker.yml"
)

# Required files for each service
declare -A REQUIRED_FILES=(
    ["api"]=".env firebase-credentials.json"
    ["worker"]=".env firebase-credentials.json youtube-cookies.txt"
)

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# -----------------------------------------------------------------------------
# Logging Functions
# -----------------------------------------------------------------------------
log_info()  { echo -e "${BLUE}[INFO]${NC} $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# -----------------------------------------------------------------------------
# Validation Functions
# -----------------------------------------------------------------------------
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (use sudo)"
        exit 1
    fi
}

check_dependencies() {
    local missing=()

    if ! command -v docker &>/dev/null; then
        missing+=("docker")
    fi

    if ! docker compose version &>/dev/null; then
        missing+=("docker-compose-v2")
    fi

    if ! command -v systemctl &>/dev/null; then
        missing+=("systemd")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing dependencies: ${missing[*]}"
        log_error "Install them first and re-run this script"
        exit 1
    fi
}

validate_service_type() {
    local service_type="$1"
    if [[ ! -v "SERVICES[$service_type]" ]]; then
        log_error "Unknown service type: $service_type"
        log_error "Valid types: ${!SERVICES[*]}"
        exit 1
    fi
}

check_required_files() {
    local service_type="$1"
    local missing=()

    for file in ${REQUIRED_FILES[$service_type]}; do
        if [[ ! -f "${APP_DIR}/${file}" ]]; then
            missing+=("$file")
        fi
    done

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required files in ${APP_DIR}:"
        for file in "${missing[@]}"; do
            log_error "  - $file"
        done
        exit 1
    fi
}

check_compose_file() {
    local service_type="$1"
    local compose_file="${APP_DIR}/deploy/${SERVICES[$service_type]}"

    if [[ ! -f "$compose_file" ]]; then
        log_error "Compose file not found: $compose_file"
        exit 1
    fi
}

check_unit_file() {
    local service_type="$1"
    local unit_file="${SCRIPT_DIR}/systemd/viralclip-${service_type}.service"

    if [[ ! -f "$unit_file" ]]; then
        log_error "Unit file not found: $unit_file"
        exit 1
    fi
}

# -----------------------------------------------------------------------------
# Service Management Functions
# -----------------------------------------------------------------------------
get_service_name() {
    local service_type="$1"
    echo "viralclip-${service_type}.service"
}

install_service() {
    local service_type="$1"
    local service_name
    service_name="$(get_service_name "$service_type")"
    local unit_src="${SCRIPT_DIR}/systemd/viralclip-${service_type}.service"
    local unit_dst="${SYSTEMD_DIR}/${service_name}"

    log_info "Installing ${service_name}..."

    # Validate prerequisites
    check_required_files "$service_type"
    check_compose_file "$service_type"
    check_unit_file "$service_type"

    # Stop existing service if running
    if systemctl is-active --quiet "$service_name" 2>/dev/null; then
        log_info "Stopping existing ${service_name}..."
        systemctl stop "$service_name"
    fi

    # Backup existing unit file if different
    if [[ -f "$unit_dst" ]]; then
        if ! diff -q "$unit_src" "$unit_dst" &>/dev/null; then
            local backup="${unit_dst}.backup.$(date +%Y%m%d%H%M%S)"
            log_info "Backing up existing unit file to ${backup}"
            cp "$unit_dst" "$backup"
        fi
    fi

    # Install unit file
    install -m 0644 "$unit_src" "$unit_dst"
    log_ok "Unit file installed: ${unit_dst}"

    # Reload systemd
    systemctl daemon-reload
    log_ok "Systemd daemon reloaded"

    # Enable service
    systemctl enable "$service_name"
    log_ok "Service enabled for auto-start on boot"

    # Start service
    log_info "Starting ${service_name}..."
    if systemctl start "$service_name"; then
        log_ok "Service started successfully"
    else
        log_error "Service failed to start. Check logs with:"
        log_error "  journalctl -u ${service_name} -n 50 --no-pager"
        exit 1
    fi

    # Show status
    echo ""
    systemctl status --no-pager "$service_name" || true
    echo ""

    # Verify container is running
    log_info "Verifying container status..."
    sleep 3
    if docker ps --format '{{.Names}}' | grep -q "vclip-${service_type}"; then
        log_ok "Container is running"
    else
        log_warn "Container may not be running yet. Check with: docker ps"
    fi
}

uninstall_service() {
    local service_type="$1"
    local service_name
    service_name="$(get_service_name "$service_type")"
    local unit_dst="${SYSTEMD_DIR}/${service_name}"

    log_info "Uninstalling ${service_name}..."

    # Stop service if running
    if systemctl is-active --quiet "$service_name" 2>/dev/null; then
        log_info "Stopping ${service_name}..."
        systemctl stop "$service_name"
        log_ok "Service stopped"
    fi

    # Disable service
    if systemctl is-enabled --quiet "$service_name" 2>/dev/null; then
        systemctl disable "$service_name"
        log_ok "Service disabled"
    fi

    # Remove unit file
    if [[ -f "$unit_dst" ]]; then
        rm -f "$unit_dst"
        log_ok "Unit file removed: ${unit_dst}"
    fi

    # Reload systemd
    systemctl daemon-reload
    log_ok "Systemd daemon reloaded"

    log_info "Note: Docker containers were not removed. To remove them:"
    log_info "  cd ${APP_DIR}"
    log_info "  docker compose -f deploy/${SERVICES[$service_type]} down"
}

show_status() {
    local service_type="$1"
    local service_name
    service_name="$(get_service_name "$service_type")"

    echo "=========================================="
    echo "Service: ${service_name}"
    echo "=========================================="
    echo ""

    # Systemd status
    echo "--- Systemd Status ---"
    if systemctl is-active --quiet "$service_name" 2>/dev/null; then
        echo "Status: ACTIVE"
    else
        echo "Status: INACTIVE"
    fi

    if systemctl is-enabled --quiet "$service_name" 2>/dev/null; then
        echo "Enabled: YES (starts on boot)"
    else
        echo "Enabled: NO"
    fi
    echo ""

    # Container status
    echo "--- Container Status ---"
    docker ps --filter "name=vclip-${service_type}" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" 2>/dev/null || echo "No containers found"
    echo ""

    # Recent logs
    echo "--- Recent Logs (last 10 lines) ---"
    journalctl -u "$service_name" -n 10 --no-pager 2>/dev/null || echo "No logs available"
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
usage() {
    cat <<EOF
Usage: $(basename "$0") <service-type> [action]

Service Types:
  api       API server service
  worker    Worker service

Actions:
  install   Install and start the service (default)
  uninstall Remove the service
  status    Show service status
  reinstall Uninstall then install

Examples:
  sudo ./install-systemd.sh api              # Install API service
  sudo ./install-systemd.sh worker           # Install Worker service
  sudo ./install-systemd.sh api uninstall    # Remove API service
  sudo ./install-systemd.sh api status       # Check status
EOF
}

main() {
    if [[ $# -lt 1 ]]; then
        usage
        exit 1
    fi

    local service_type="$1"
    local action="${2:-install}"

    # Handle help flag
    if [[ "$service_type" == "-h" || "$service_type" == "--help" ]]; then
        usage
        exit 0
    fi

    # Validate inputs
    check_root
    check_dependencies
    validate_service_type "$service_type"

    # Execute action
    case "$action" in
        install)
            install_service "$service_type"
            ;;
        uninstall)
            uninstall_service "$service_type"
            ;;
        status)
            show_status "$service_type"
            ;;
        reinstall)
            uninstall_service "$service_type"
            install_service "$service_type"
            ;;
        *)
            log_error "Unknown action: $action"
            usage
            exit 1
            ;;
    esac
}

main "$@"
