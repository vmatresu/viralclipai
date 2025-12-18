#!/bin/bash
# =============================================================================
# ViralClip AI - Unified Server Setup & Hardening
# =============================================================================
# One-step setup for fresh Ubuntu servers (OVH, DigitalOcean, Hetzner, etc.)
# Handles:
# 1. System Hardening (UFW, Fail2ban, Auto-updates, SSH Security)
# 2. Dependencies (Docker, Compose, git, jq)
# 3. Application Setup (User, Directory, Permissions)
# 4. Environment Security (Auto-generates Secrets for Redis/JWT)
#
# Usage:
#   # Run as root on a fresh server
#   sudo ./setup-server.sh            # Standard (API + Nginx + Firewall ports 80/443)
#   sudo ./setup-server.sh --worker   # Worker Only (No Nginx, Firewall only SSH)
# =============================================================================

set -euo pipefail

# Parse Arguments
SERVER_TYPE="full"
for arg in "$@"; do
    case $arg in
        --worker)
            SERVER_TYPE="worker"
            shift
            ;;
    esac
done

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
APP_DIR="/var/www/viralclipai-backend"
DEPLOY_USER="deploy"
SSH_PORT="22"

log_info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok()    { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root"
        exit 1
    fi
}

# =============================================================================
# 1. System Updates & Dependencies
# =============================================================================
step_updates() {
    log_info "Updating system packages..."
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get upgrade -y
    
    # Base dependencies
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
        ufw fail2ban unattended-upgrades apt-listchanges auditd \
        curl wget git jq ca-certificates gnupg lsb-release

    # Web server dependencies (Only for API/Full nodes)
    if [[ "$SERVER_TYPE" != "worker" ]]; then
        log_info "Installing Nginx and Certbot..."
        DEBIAN_FRONTEND=noninteractive apt-get install -y \
            nginx certbot python3-certbot-nginx
    else
        log_info "Worker node: Skipping Nginx installation."
    fi

    log_ok "System updated and dependencies installed."
}

# =============================================================================
# 2. Docker Installation
# =============================================================================
step_docker() {
    if ! command -v docker &> /dev/null; then
        log_info "Installing Docker..."
        mkdir -p /etc/apt/keyrings
        curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
        echo \
          "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
          $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null
        
        apt-get update
        DEBIAN_FRONTEND=noninteractive apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
    fi

    # Docker Hardening
    mkdir -p /etc/docker
    cat > /etc/docker/daemon.json << EOF
{
    "log-driver": "json-file",
    "log-opts": {
        "max-size": "50m",
        "max-file": "3"
    },
    "live-restore": true,
    "userland-proxy": false,
    "no-new-privileges": true
}
EOF
    systemctl restart docker
    log_ok "Docker installed and configured."
}

# =============================================================================
# 3. Kernel & Network Hardening (Speed + Security)
# =============================================================================
step_kernel() {
    log_info "Applying Kernel Hardening & Performance Tweaks..."
    
    cat > /etc/sysctl.d/99-viralclip-hardening.conf << EOF
# --- Network Security ---
# IP Spoofing protection
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1

# Ignore ICMP broadcast requests
net.ipv4.icmp_echo_ignore_broadcasts = 1

# Disable source packet routing
net.ipv4.conf.all.accept_source_route = 0
net.ipv6.conf.all.accept_source_route = 0

# Block SYN attacks (SYN Cookies)
net.ipv4.tcp_syncookies = 1
net.ipv4.tcp_max_syn_backlog = 2048
net.ipv4.tcp_synack_retries = 2

# Log Martians
net.ipv4.conf.all.log_martians = 1

# --- Performance (TCP BBR) ---
# Enable BBR Congestion Control for higher throughput/lower latency
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr

# Increase system file descriptor limit
fs.file-max = 65535
net.core.somaxconn = 65535
EOF

    sysctl --system
    log_ok "Kernel parameters applied (BBR enabled)."
}

# =============================================================================
# 4. User & SSH Hardening
# =============================================================================
step_user_security() {
    # Create deploy user
    if ! id "$DEPLOY_USER" &>/dev/null; then
        log_info "Creating user: $DEPLOY_USER"
        useradd -m -s /bin/bash -G sudo,docker "$DEPLOY_USER"
    else
        log_warn "User $DEPLOY_USER exists. Ensuring docker group membership..."
        usermod -aG docker "$DEPLOY_USER"
    fi

    # Setup SSH Keys (Copy from root if available)
    mkdir -p /home/$DEPLOY_USER/.ssh
    if [[ -f /root/.ssh/authorized_keys ]]; then
        cp /root/.ssh/authorized_keys /home/$DEPLOY_USER/.ssh/authorized_keys
        chown -R $DEPLOY_USER:$DEPLOY_USER /home/$DEPLOY_USER/.ssh
        chmod 700 /home/$DEPLOY_USER/.ssh
        chmod 600 /home/$DEPLOY_USER/.ssh/authorized_keys
    else
        log_warn "No root SSH keys found to copy. Please add keys manually to /home/$DEPLOY_USER/.ssh/authorized_keys"
    fi

    # Passwordless Sudo for Docker (convenience for deployment)
    echo "$DEPLOY_USER ALL=(ALL) NOPASSWD: /usr/bin/docker, /usr/bin/docker-compose, /usr/bin/systemctl restart docker" \
        > /etc/sudoers.d/$DEPLOY_USER
    chmod 440 /etc/sudoers.d/$DEPLOY_USER

    # Hardening SSHD
    log_info "Hardening SSH config..."
    cp /etc/ssh/sshd_config /etc/ssh/sshd_config.backup
    cat > /etc/ssh/sshd_config.d/hardening.conf << EOF
Port $SSH_PORT
Protocol 2
PermitRootLogin no
PasswordAuthentication no
PermitEmptyPasswords no
ChallengeResponseAuthentication no
UsePAM yes
PubkeyAuthentication yes
AllowUsers $DEPLOY_USER
X11Forwarding no
MaxAuthTries 3

# Modern Crypto (2025 Standards)
KexAlgorithms curve25519-sha256,curve25519-sha256@libssh.org,diffie-hellman-group16-sha512,diffie-hellman-group18-sha512,diffie-hellman-group-exchange-sha256
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com,aes256-ctr,aes192-ctr,aes128-ctr
MACs hmac-sha2-256-etm@openssh.com,hmac-sha2-512-etm@openssh.com,hmac-sha2-256,hmac-sha2-512
EOF
    
    # We don't restart SSH yet to avoid locking you out if keys aren't set up
    log_ok "SSH configuration updated (restart service manually after verification)."
}

# =============================================================================
# 4. Firewall (UFW) & Fail2Ban
# =============================================================================
step_firewall() {
    log_info "Configuring Firewall..."
    ufw default deny incoming
    ufw default allow outgoing
    ufw allow $SSH_PORT/tcp comment 'SSH'
    
    if [[ "$SERVER_TYPE" != "worker" ]]; then
        ufw allow 80/tcp comment 'HTTP'
        ufw allow 443/tcp comment 'HTTPS'
    else
        log_info "Worker node: Keeping HTTP/HTTPS ports closed."
    fi

    # Redis is NOT allowed externally (safe default)
    
    # Enable UFW non-interactively
    echo "y" | ufw enable
    log_ok "Firewall enabled."

    log_info "Configuring Fail2Ban..."
    cat > /etc/fail2ban/jail.local << EOF
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 3
backend = systemd
banaction = ufw

[sshd]
enabled = true
port = $SSH_PORT
EOF
    systemctl restart fail2ban
    log_ok "Fail2Ban configured."
}

# =============================================================================
# 5. Application Environment Setup (The Magic Part)
# =============================================================================
step_app_env() {
    log_info "Setting up Application Environment..."
    mkdir -p "$APP_DIR"
    
    # Ensure correct permissions
    chown -R $DEPLOY_USER:$DEPLOY_USER "$APP_DIR"
    chmod 750 "$APP_DIR"

    ENV_FILE="$APP_DIR/.env"
    
    # Check if .env exists
    if [[ -f "$ENV_FILE" ]]; then
        log_warn ".env file already exists. Checking for missing secrets..."
        # If REDIS_PASSWORD is missing, append it
        if ! grep -q "REDIS_PASSWORD=" "$ENV_FILE"; then
            PASS=$(openssl rand -hex 32)
            echo "REDIS_PASSWORD=$PASS" >> "$ENV_FILE"
            log_ok "Added missing REDIS_PASSWORD to .env"
        fi
        
        # If JWT_SECRET is missing, append it
        if ! grep -q "JWT_SECRET=" "$ENV_FILE"; then
            SECRET=$(openssl rand -hex 32)
            echo "JWT_SECRET=$SECRET" >> "$ENV_FILE"
            log_ok "Added missing JWT_SECRET to .env"
        fi
    else
        log_info "Creating new .env file with generated secrets..."
        
        # Generate secure secrets
        REDIS_PASS=$(openssl rand -hex 32)
        JWT_SECRET=$(openssl rand -hex 32)
        
        cat > "$ENV_FILE" << EOF
# ======================================
# ViralClip AI - Production Configuration
# Auto-generated by setup-server.sh
# ======================================

ENVIRONMENT=production
RUST_LOG=info

# --- Secrets (Auto-Generated) ---
REDIS_PASSWORD=$REDIS_PASS
JWT_SECRET=$JWT_SECRET

# --- API Configuration ---
API_PORT=8000
API_HOST=0.0.0.0

# --- Worker Configuration ---
WORKER_CONCURRENCY=4

# --- External Services (YOU MUST FILL THESE) ---
# Firebase
FIREBASE_PROJECT_ID=viralclipai-prod

# Cloudflare R2
R2_ACCOUNT_ID=
R2_ACCESS_KEY_ID=
R2_SECRET_ACCESS_KEY=
R2_BUCKET_NAME=viralclip-media
R2_ENDPOINT_URL=https://<account_id>.r2.cloudflarestorage.com

# Gemini AI
GEMINI_API_KEY=

EOF
        chmod 600 "$ENV_FILE"
        chown $DEPLOY_USER:$DEPLOY_USER "$ENV_FILE"
        log_ok "New .env file created at $ENV_FILE"
        log_warn "ACTION REQUIRED: You must edit $ENV_FILE to add R2 keys and Firebase ID!"
    fi
}

# =============================================================================
# Main Execution
# =============================================================================
check_root

echo "Starting setup..."
step_updates
step_docker
step_kernel
step_user_security
step_firewall
step_app_env

log_info "--------------------------------------------------------"
log_info "Setup Complete!"
log_info "1. Verify SSH login: ssh $DEPLOY_USER@<ip>"
log_info "2. Edit config: nano $APP_DIR/.env"
log_info "3. Deploy code: git clone ... or use GitHub Actions"
log_info "--------------------------------------------------------"
