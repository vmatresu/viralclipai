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
REDIS_PUBLIC=false
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
            --redis-public) REDIS_PUBLIC=true; shift ;;
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
    # Base command in unit: --file deploy/docker-compose.<role>.yml
    local compose_files="--file deploy/docker-compose.${ROLE}.yml"
    
    # Add Redis compose file if requested
    if [[ "$WITH_REDIS" == "true" ]]; then
        compose_files="$compose_files --file deploy/docker-compose.redis.yml"
    fi
    
    # 2. Customize Unit File (Inject compose flags)
    # We copy to tmp, replace the default flags, then install
    cp "$source_unit" "/tmp/$service_name"
    
    # Replace the hardcoded '--file deploy/docker-compose.api.yml' with our dynamic list
    # The unit file has: "--file deploy/docker-compose.api.yml" (check systemd/viralclip-api.service)
    sed -i "s|--file deploy/docker-compose.${ROLE}.yml|$compose_files|g" "/tmp/$service_name"
    
    # 3. Install & Start
    install -m 0644 "/tmp/$service_name" "$dest_unit"
    rm "/tmp/$service_name"
    
    systemctl daemon-reload
    systemctl enable "$service_name"
    systemctl restart "$service_name"
    
    success "Systemd service ($service_name) installed & started"
    echo ""
    echo "------------------------------------------------------------------"
    echo "⚠️  NOTE: The initial build can take 10-20 minutes."
    echo "    The service is running in the background (systemd)."
    echo "    It is SAFE to disconnect your SSH session now."
    echo ""
    echo "    To watch the build progress live, run:"
    echo "    sudo journalctl -u $service_name -f"
    echo "------------------------------------------------------------------"
}

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

configure_app_files() {
    # 1. Firebase Credentials (Required for both API and Worker)
    local fb_creds="$APP_DIR/firebase-credentials.json"
    if [[ ! -f "$fb_creds" ]]; then
        if [[ -d "$fb_creds" ]]; then
            log "Removing invalid directory: $fb_creds"
            rmdir "$fb_creds"
        fi
        log "Creating empty firebase-credentials.json (Placeholder)..."
        echo "{}" > "$fb_creds"
        chmod 600 "$fb_creds"
        chown deploy:deploy "$fb_creds"
        echo "⚠️  WARNING: You must upload your real firebase-credentials.json to $APP_DIR!"
    fi

    # 2. Redis Configuration (If Redis enabled)
    if [[ "$WITH_REDIS" == "true" ]]; then
        local redis_conf="$DEPLOY_DIR/redis/redis.conf"
        if [[ -d "$redis_conf" ]]; then
            log "Found directory at $redis_conf (Docker mount artifact). Removing..."
            rmdir "$redis_conf"
        fi
        
        if [[ ! -f "$redis_conf" ]]; then
            error "Redis config missing at $redis_conf. Please 'git pull' or restore the file."
        fi

        # Handle Redis Bind IP (Public vs Private)
        if [[ "$REDIS_PUBLIC" == "true" ]]; then
            log "Configuring Redis for PUBLIC access (0.0.0.0)..."
            # Ensure REDIS_BIND_IP exists in .env, if not append it
            if ! grep -q "REDIS_BIND_IP=" "$APP_DIR/.env"; then
                echo "REDIS_BIND_IP=0.0.0.0" >> "$APP_DIR/.env"
            else
                sed -i 's/^REDIS_BIND_IP=.*/REDIS_BIND_IP=0.0.0.0/' "$APP_DIR/.env"
            fi
        fi
    fi

    # Note: YouTube cookies are optional
    # If you need them for age-restricted content, upload a valid Netscape format cookies file:
    #   scp cookies.txt deploy@<SERVER_IP>:/var/www/viralclipai-backend/youtube-cookies.txt
}

# --- Main ---

check_root
parse_args "$@"

echo "=================================================="
echo " Provisioning ViralClip AI ($ROLE)"
echo "=================================================="

configure_app_files
configure_nginx
configure_firewall
install_systemd
setup_ssl

echo "=================================================="
success "Deployment Complete!"
echo "=================================================="

# Final Checks & Warnings
echo ""
echo -e "${YELLOW}!!! MANUAL ACTION REQUIRED !!!${NC}"
echo "--------------------------------------------------"
echo "1. Edit .env file:"
echo "   - Ensure R2 Keys, Firebase ID, and Redis URL are correct."
echo "   - File: $APP_DIR/.env"
echo ""
echo "2. Upload Firebase Credentials:"
echo "   - I created a dummy file to prevent crashes."
echo "   - You MUST overwrite it with your real file:"
echo "     scp your-credentials.json deploy@<SERVER_IP>:$APP_DIR/firebase-credentials.json"
echo ""

if [[ "$ROLE" == "worker" ]]; then
    echo "3. (Optional) YouTube Cookies for age-restricted content:"
    echo "   - Only needed if downloading age-restricted videos."
    echo "   - Must be in Netscape format (from browser extension)."
    echo "   - Upload if needed:"
    echo "     scp cookies.txt deploy@<SERVER_IP>:$APP_DIR/youtube-cookies.txt"
fi
echo "--------------------------------------------------"
echo ""
