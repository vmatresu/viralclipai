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
    DEBIAN_FRONTEND=noninteractive apt-get dist-upgrade -y
    DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y
    apt-get autoclean
    
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
# IP Spoofing protection (Set to 2 for loose mode, avoids lockouts on OVH/Cloud)
net.ipv4.conf.all.rp_filter = 2
net.ipv4.conf.default.rp_filter = 2

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

    # Setup SSH Keys for deploy user
    # Cloud providers (OVH, etc.) often add command restrictions to root's keys
    # We prefer ubuntu user's keys (clean), then fall back to root with sanitization
    mkdir -p /home/$DEPLOY_USER/.ssh
    chmod 700 /home/$DEPLOY_USER/.ssh

    if [[ -f /home/ubuntu/.ssh/authorized_keys ]]; then
        # Ubuntu user exists (OVH default) - use their clean keys
        cp /home/ubuntu/.ssh/authorized_keys /home/$DEPLOY_USER/.ssh/authorized_keys
        log_info "Copied SSH keys from ubuntu user"
    elif [[ -f /root/.ssh/authorized_keys ]]; then
        # No ubuntu user - extract keys from root, stripping command= restrictions
        grep -oE '(ssh-rsa|ssh-ed25519|ecdsa-sha2-nistp[0-9]+) [A-Za-z0-9+/=]+ ?[^ ]*' \
            /root/.ssh/authorized_keys > /home/$DEPLOY_USER/.ssh/authorized_keys 2>/dev/null || true
        log_info "Extracted SSH keys from root (stripped any command restrictions)"
    else
        log_warn "No SSH keys found. Please add keys manually to /home/$DEPLOY_USER/.ssh/authorized_keys"
    fi

    if [[ -f /home/$DEPLOY_USER/.ssh/authorized_keys ]]; then
        chown -R $DEPLOY_USER:$DEPLOY_USER /home/$DEPLOY_USER/.ssh
        chmod 600 /home/$DEPLOY_USER/.ssh/authorized_keys
        log_ok "SSH keys configured for $DEPLOY_USER"
    fi

    # Passwordless Sudo for ALL commands (requested by user)
    echo "$DEPLOY_USER ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/$DEPLOY_USER
    chmod 440 /etc/sudoers.d/$DEPLOY_USER

    # Hardening SSHD (Ubuntu 24.04 / OpenSSH 9.x compatible)
    # Based on ssh-audit.com recommendations (April 2025)
    log_info "Hardening SSH config..."
    cp /etc/ssh/sshd_config /etc/ssh/sshd_config.backup

    # Re-generate ED25519 host key if missing (most secure)
    if [[ ! -f /etc/ssh/ssh_host_ed25519_key ]]; then
        ssh-keygen -t ed25519 -f /etc/ssh/ssh_host_ed25519_key -N ""
    fi

    # Create privilege separation directory (required by sshd)
    # Also create tmpfiles.d config so it persists across reboots (/run is tmpfs)
    mkdir -p /run/sshd
    chmod 755 /run/sshd
    echo "d /run/sshd 0755 root root -" > /etc/tmpfiles.d/sshd.conf

    cat > /etc/ssh/sshd_config.d/hardening.conf << EOF
# =============================================================================
# SSH Hardening - Ubuntu 24.04 LTS (OpenSSH 9.x)
# Based on ssh-audit.com recommendations (2025)
# =============================================================================

# --- Basic Security ---
Port $SSH_PORT
PermitRootLogin no
PasswordAuthentication no
PermitEmptyPasswords no
KbdInteractiveAuthentication no
UsePAM yes
PubkeyAuthentication yes
AuthenticationMethods publickey
AllowUsers ubuntu $DEPLOY_USER
X11Forwarding no
MaxAuthTries 3
MaxSessions 10
LoginGraceTime 30
ClientAliveInterval 300
ClientAliveCountMax 2

# --- Crypto (2025 Quantum-Resistant Standards) ---
# Key Exchange: Prefer post-quantum hybrid, then modern curves
KexAlgorithms sntrup761x25519-sha512@openssh.com,curve25519-sha256,curve25519-sha256@libssh.org,diffie-hellman-group18-sha512,diffie-hellman-group-exchange-sha256,diffie-hellman-group16-sha512

# Ciphers: 256-bit preferred for quantum resistance
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes256-ctr,aes192-ctr,aes128-gcm@openssh.com,aes128-ctr

# MACs: ETM (Encrypt-then-MAC) modes only
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,umac-128-etm@openssh.com

# Host Keys: Prefer ED25519
HostKeyAlgorithms ssh-ed25519,ssh-ed25519-cert-v01@openssh.com,sk-ssh-ed25519@openssh.com,sk-ssh-ed25519-cert-v01@openssh.com,rsa-sha2-512,rsa-sha2-512-cert-v01@openssh.com,rsa-sha2-256,rsa-sha2-256-cert-v01@openssh.com

# Accept all standard pubkey algorithms (including ssh-rsa for compatibility)
PubkeyAcceptedAlgorithms +ssh-rsa
EOF

    # Ubuntu 24.04: Ensure SSH is enabled (Socket activation is default)
    systemctl enable ssh.service || true
    systemctl enable ssh.socket || true

    systemctl daemon-reload
    systemctl start ssh.socket
    systemctl start ssh.service

    # Test config before applying
    if sshd -t; then
        log_ok "SSH configuration valid"
    else
        log_error "SSH configuration invalid! Reverting..."
        rm -f /etc/ssh/sshd_config.d/hardening.conf
        exit 1
    fi

    # Verify SSH is enabled for boot
    if systemctl is-enabled ssh.service &>/dev/null || systemctl is-enabled ssh.socket &>/dev/null; then
        log_ok "SSH is enabled to start on boot"
    else
        log_warn "SSH might not start on boot. checking..."
        systemctl enable ssh.service || true
    fi

    log_ok "SSH hardening applied. Service will be restarted at end of setup."
}

# =============================================================================
# 5. Git & Private Repo Setup
# =============================================================================
step_git_setup() {
    log_info "Configuring Git Access for Private Repo..."
    
    # 1. Generate SSH Key for GitHub (Deploy Key)
    local key_file="/home/$DEPLOY_USER/.ssh/id_ed25519"
    if [[ ! -f "$key_file" ]]; then
        log_info "Generating SSH key for GitHub access..."
        su - "$DEPLOY_USER" -c "ssh-keygen -t ed25519 -f $key_file -N '' -C 'deploy@viralclipai'"
    fi

    # 2. Add GitHub to known_hosts (prevent interactive prompt)
    if ! grep -q "github.com" "/home/$DEPLOY_USER/.ssh/known_hosts" 2>/dev/null; then
        log_info "Adding github.com to known_hosts..."
        su - "$DEPLOY_USER" -c "ssh-keyscan github.com >> ~/.ssh/known_hosts 2>/dev/null"
    fi
    
    log_ok "Git access configured."
}

# =============================================================================
# 6. Firewall (UFW) & Fail2Ban
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
step_git_setup
step_firewall
step_app_env

# Restart SSH to apply hardening (do this last, after all config is done)
log_info "Restarting SSH to apply hardening..."

# Ensure /run/sshd exists (may be cleared on reboot)
mkdir -p /run/sshd
chmod 755 /run/sshd

# Stop everything first to avoid "address already in use"
systemctl stop ssh.socket 2>/dev/null || true
systemctl stop ssh.service 2>/dev/null || true
sleep 1

# Start ssh.service (this is more reliable than socket activation)
systemctl start ssh.service

# Verify SSH is running
if systemctl is-active --quiet ssh.service; then
    log_ok "SSH service is running"
else
    log_error "SSH service failed! Check: journalctl -xeu ssh.service"
fi

# Get Public Key for display
PUB_KEY=$(cat /home/$DEPLOY_USER/.ssh/id_ed25519.pub)

log_info "--------------------------------------------------------"
log_info "Setup Complete!"
log_info "--------------------------------------------------------"
echo -e "${YELLOW}1. GIT ACCESS (Private Repo):${NC}"
echo "   Go to your GitHub Repo -> Settings -> Deploy Keys -> Add Deploy Key"
echo "   Paste this key (Allow write access if needed, usually Read-only is fine):"
echo ""
echo -e "${GREEN}${PUB_KEY}${NC}"
echo ""
log_info "--------------------------------------------------------"
echo -e "${YELLOW}2. GITHUB ACTIONS ACCESS:${NC}"
echo "   To allow GitHub Actions to deploy, add its SSH Public Key to authorized_keys:"
echo "   Command: echo 'YOUR_GITHUB_ACTIONS_PUBLIC_KEY' >> /home/$DEPLOY_USER/.ssh/authorized_keys"
echo ""
log_info "--------------------------------------------------------"
log_info "3. Verify SSH login (new terminal): ssh $DEPLOY_USER@<ip>"
log_info "4. Deploy code:"
echo "   ssh $DEPLOY_USER@<ip>"
echo "   git clone git@github.com:valentin/viralclipai.git $APP_DIR"
echo "   cd $APP_DIR"
echo "   sudo ./deploy/provision.sh --role [api|worker]"
log_info "--------------------------------------------------------"
log_warn "DO NOT close this terminal until you verify SSH works!"
