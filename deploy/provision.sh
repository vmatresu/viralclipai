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
    if [[ -z "$ROLE" ]]; then
        error "Missing --role <api|worker>"
    fi
}

configure_nginx() {
    [[ "$ROLE" != "api" ]] && return
    log "Configuring Nginx..."
    
    # Check if we need to bootstrap SSL
    local cert_path="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"
    
    if [[ -n "$DOMAIN" && ! -f "$cert_path" ]]; then
        log "SSL certs missing. Using temporary HTTP config for Certbot..."
        
        # Create temp config for ACME challenge
        cat > /etc/nginx/nginx.conf <<EOF
events {}
http {
    server {
        listen 80;
        server_name $DOMAIN;
        location /.well-known/acme-challenge/ {
            root /var/www/certbot;
        }
        location / {
            return 200 'Bootstrap SSL...';
        }
    }
}
EOF
        mkdir -p /var/www/certbot
        nginx -t && systemctl reload nginx || error "Temp Nginx config failed"
        return
    fi
    
    # Standard Config Apply
    cp "$DEPLOY_DIR/nginx/nginx.conf" /etc/nginx/nginx.conf
    mkdir -p /var/log/nginx
    nginx -t && systemctl reload nginx || error "Nginx config failed. Check if SSL certs exist."
    success "Nginx configured"
}

# ... (rest of script)

setup_ssl() {
    [[ -z "$DOMAIN" ]] && return
    [[ "$ROLE" != "api" ]] && return
    
    # Check if already has certs
    if [[ -f "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" ]]; then
        log "SSL Certificates already exist. Skipping Certbot."
        return
    fi
    
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
    
    # Apply the real Nginx config now that we have certs
    log "Applying production Nginx config..."
    configure_nginx
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
