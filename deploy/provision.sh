#!/bin/bash
# =============================================================================
# ViralClip AI - Application Provisioning
# =============================================================================
# The ONLY script needed to configure the application after cloning.
# It handles Systemd, Nginx, Redis, Firewall, and SSL in one go.
#
# Usage:
#   # API Server (with Local Redis & SSL)
#   sudo ./deploy/provision.sh --role api --redis --worker-ip 57.128.55.191 --domain api.viralclipai.io --email admin@example.com
#
#   # Worker Server (Connects to API's Redis)
#   sudo ./deploy/provision.sh --role worker
# =============================================================================

set -euo pipefail

# --- Configuration ---
APP_DIR="/var/www/viralclipai-backend"
DEPLOY_DIR="$APP_DIR/deploy"
SYSTEMD_DIR="/etc/systemd/system"

# Flags
ROLE=""
WITH_REDIS=false
WORKER_IP=""
DOMAIN=""
EMAIL=""

# Colors
BLUE='\033[0;34m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

log() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# --- Helper Functions ---

check_root() {
    [[ $EUID -eq 0 ]] || error "Must run as root (sudo)"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --role) ROLE="$2"; shift 2 ;;
            --redis) WITH_REDIS=true; shift ;;
            --worker-ip) WORKER_IP="$2"; shift 2 ;;
            --domain) DOMAIN="$2"; shift 2 ;;
            --email) EMAIL="$2"; shift 2 ;;
            *) error "Unknown argument: $1" ;;
        esac
    done
    [[ -z "$ROLE" ]] && error "Missing --role <api|worker>"
}

configure_nginx() {
    [[ "$ROLE" != "api" ]] && return
    log "Configuring Nginx..."
    
    cp "$DEPLOY_DIR/nginx/nginx.conf" /etc/nginx/nginx.conf
    mkdir -p /var/log/nginx
    nginx -t && systemctl reload nginx || error "Nginx config failed"
    success "Nginx configured"
}

configure_firewall() {
    if [[ "$WITH_REDIS" == "true" ]]; then
        log "Opening Redis port (6379)..."
        ufw allow from 127.0.0.1 to any port 6379 comment 'Redis Local' >/dev/null
        
        if [[ -n "$WORKER_IP" ]]; then
            ufw allow from "$WORKER_IP" to any port 6379 comment 'Redis from Worker'
            success "Firewall: Allowed Worker ($WORKER_IP) to Redis"
        else
            echo "⚠️  Redis enabled but no Worker IP provided (--worker-ip). Only localhost can connect."
        fi
    fi
}

install_systemd() {
    log "Installing Systemd Service..."
    
    local service_name="viralclip-${ROLE}.service"
    local source_unit="$DEPLOY_DIR/systemd/${service_name}"
    local dest_unit="$SYSTEMD_DIR/${service_name}"
    
    [[ -f "$source_unit" ]] || error "Unit file not found: $source_unit"
    
    # 1. Prepare Docker Compose Command
    # Base command: docker compose -f deploy/docker-compose.<role>.yml
    local compose_files="-f deploy/docker-compose.${ROLE}.yml"
    
    # Add Redis compose file if requested
    if [[ "$WITH_REDIS" == "true" ]]; then
        compose_files="$compose_files -f deploy/docker-compose.redis.yml"
    fi
    
    # 2. Customize Unit File (Inject compose flags)
    # We copy to tmp, replace the default flags, then install
    cp "$source_unit" "/tmp/$service_name"
    
    # Replace the hardcoded '-f deploy/docker-compose.api.yml' (or worker) with our dynamic list
    # We use | as delimiter to avoid issues with paths
    sed -i "s| -f deploy/docker-compose.${ROLE}.yml| $compose_files|g" "/tmp/$service_name"
    
    # 3. Install & Start
    install -m 0644 "/tmp/$service_name" "$dest_unit"
    rm "/tmp/$service_name"
    
    systemctl daemon-reload
    systemctl enable "$service_name"
    systemctl restart "$service_name"
    
    success "Systemd service ($service_name) installed & started"
}

setup_ssl() {
    [[ -z "$DOMAIN" ]] && return
    [[ "$ROLE" != "api" ]] && return
    
    log "Setting up SSL for $DOMAIN..."
    [[ -z "$EMAIL" ]] && error "Email required for SSL (--email)"
    
    # Create webroot for validation
    mkdir -p /var/www/certbot
    
    # Certbot Command
    certbot certonly \
        --webroot \
        --webroot-path=/var/www/certbot \
        --email "$EMAIL" \
        --agree-tos \
        --no-eff-email \
        --non-interactive \
        -d "$DOMAIN" || error "Certbot failed"
        
    success "SSL Certificate obtained"
    log "Restarting Nginx to apply..."
    systemctl reload nginx
}

# --- Main ---

check_root
parse_args "$@"

echo "=================================================="
echo " Provisioning ViralClip AI ($ROLE)"
echo "=================================================="

configure_nginx
configure_firewall
install_systemd
setup_ssl

echo "=================================================="
success "Deployment Complete!"
echo "=================================================="
