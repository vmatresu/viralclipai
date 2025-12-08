#!/bin/bash
# =============================================================================
# Server Hardening Script for Ubuntu 24.04
# =============================================================================
# Run as root on a fresh Digital Ocean droplet
# Usage: sudo ./server-hardening.sh <deploy_username>
# =============================================================================
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Validate running as root
if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root"
    exit 1
fi

# Get deploy username
DEPLOY_USER="${1:-deploy}"
SSH_PORT="${2:-22}"

log_info "Starting server hardening for Ubuntu 24.04"
log_info "Deploy user: $DEPLOY_USER"
log_info "SSH port: $SSH_PORT"

# =============================================================================
# 1. System Updates
# =============================================================================
log_info "Updating system packages..."
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get upgrade -y
DEBIAN_FRONTEND=noninteractive apt-get dist-upgrade -y
apt-get autoremove -y

# =============================================================================
# 2. Install Required Packages
# =============================================================================
log_info "Installing security packages..."
DEBIAN_FRONTEND=noninteractive apt-get install -y \
    ufw \
    fail2ban \
    unattended-upgrades \
    apt-listchanges \
    auditd \
    acl \
    curl \
    wget \
    git \
    jq \
    nginx \
    certbot \
    python3-certbot-nginx

# =============================================================================
# 3. Create Deploy User
# =============================================================================
if id "$DEPLOY_USER" &>/dev/null; then
    log_warn "User $DEPLOY_USER already exists"
else
    log_info "Creating deploy user: $DEPLOY_USER"
    useradd -m -s /bin/bash -G sudo "$DEPLOY_USER"
    
    # Copy SSH keys from root to deploy user
    mkdir -p /home/$DEPLOY_USER/.ssh
    if [[ -f /root/.ssh/authorized_keys ]]; then
        cp /root/.ssh/authorized_keys /home/$DEPLOY_USER/.ssh/
    fi
    chown -R $DEPLOY_USER:$DEPLOY_USER /home/$DEPLOY_USER/.ssh
    chmod 700 /home/$DEPLOY_USER/.ssh
    chmod 600 /home/$DEPLOY_USER/.ssh/authorized_keys 2>/dev/null || true
fi

# Allow deploy user passwordless sudo for docker commands
echo "$DEPLOY_USER ALL=(ALL) NOPASSWD: /usr/bin/docker, /usr/bin/docker-compose, /usr/bin/systemctl restart docker" \
    > /etc/sudoers.d/$DEPLOY_USER
chmod 440 /etc/sudoers.d/$DEPLOY_USER

# =============================================================================
# 4. SSH Hardening
# =============================================================================
log_info "Hardening SSH configuration..."

# Backup original config
cp /etc/ssh/sshd_config /etc/ssh/sshd_config.backup

cat > /etc/ssh/sshd_config.d/hardening.conf << EOF
# SSH Hardening Configuration
Port $SSH_PORT
Protocol 2

# Authentication
PermitRootLogin no
PasswordAuthentication no
PermitEmptyPasswords no
ChallengeResponseAuthentication no
UsePAM yes
PubkeyAuthentication yes

# Limit users
AllowUsers $DEPLOY_USER

# Security settings  
X11Forwarding no
MaxAuthTries 3
MaxSessions 3
ClientAliveInterval 300
ClientAliveCountMax 2
LoginGraceTime 30

# Disable weak algorithms
KexAlgorithms curve25519-sha256@libssh.org,diffie-hellman-group-exchange-sha256
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com

# Logging
LogLevel VERBOSE
SyslogFacility AUTH
EOF

# Validate SSH config before restarting
sshd -t && log_info "SSH configuration valid" || { log_error "Invalid SSH config!"; exit 1; }

# =============================================================================
# 5. Firewall Configuration (UFW)
# =============================================================================
log_info "Configuring firewall (UFW)..."

# Reset UFW to default
ufw --force reset

# Default policies
ufw default deny incoming
ufw default allow outgoing

# Allow SSH (custom port if changed)
ufw allow $SSH_PORT/tcp comment 'SSH'

# Allow HTTP/HTTPS
ufw allow 80/tcp comment 'HTTP'
ufw allow 443/tcp comment 'HTTPS'

# Allow internal Docker networks (optional, for inter-container communication)
# ufw allow from 172.16.0.0/12 comment 'Docker internal'

# Enable UFW
ufw --force enable
ufw status verbose

# =============================================================================
# 6. Fail2ban Configuration
# =============================================================================
log_info "Configuring fail2ban..."

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
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
bantime = 86400

[nginx-limit-req]
enabled = true
filter = nginx-limit-req
port = http,https
logpath = /var/log/nginx/error.log
maxretry = 10
bantime = 3600

[nginx-botsearch]
enabled = true
filter = nginx-botsearch
port = http,https
logpath = /var/log/nginx/access.log
maxretry = 2
bantime = 86400
EOF

systemctl enable fail2ban
systemctl restart fail2ban

# =============================================================================
# 7. Automatic Security Updates
# =============================================================================
log_info "Configuring unattended-upgrades..."

cat > /etc/apt/apt.conf.d/50unattended-upgrades << EOF
Unattended-Upgrade::Allowed-Origins {
    "\${distro_id}:\${distro_codename}";
    "\${distro_id}:\${distro_codename}-security";
    "\${distro_id}ESMApps:\${distro_codename}-apps-security";
    "\${distro_id}ESM:\${distro_codename}-infra-security";
};

Unattended-Upgrade::Package-Blacklist {
};

Unattended-Upgrade::DevRelease "false";
Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::MinimalSteps "true";
Unattended-Upgrade::Remove-Unused-Kernel-Packages "true";
Unattended-Upgrade::Remove-New-Unused-Dependencies "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "false";
Unattended-Upgrade::Automatic-Reboot-Time "03:00";
EOF

cat > /etc/apt/apt.conf.d/20auto-upgrades << EOF
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
APT::Periodic::Download-Upgradeable-Packages "1";
EOF

systemctl enable unattended-upgrades
systemctl restart unattended-upgrades

# =============================================================================
# 8. Kernel Security Parameters
# =============================================================================
log_info "Configuring kernel security parameters..."

cat > /etc/sysctl.d/99-security.conf << EOF
# IP Spoofing protection
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1

# Ignore ICMP broadcast requests
net.ipv4.icmp_echo_ignore_broadcasts = 1

# Disable source packet routing
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.conf.default.accept_source_route = 0
net.ipv6.conf.all.accept_source_route = 0
net.ipv6.conf.default.accept_source_route = 0

# Ignore send redirects
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0

# Block SYN attacks
net.ipv4.tcp_syncookies = 1
net.ipv4.tcp_max_syn_backlog = 2048
net.ipv4.tcp_synack_retries = 2
net.ipv4.tcp_syn_retries = 5

# Log Martians
net.ipv4.conf.all.log_martians = 1
net.ipv4.conf.default.log_martians = 1

# Ignore ICMP redirects
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv6.conf.all.accept_redirects = 0
net.ipv6.conf.default.accept_redirects = 0

# Ignore Directed pings
net.ipv4.icmp_echo_ignore_all = 0

# Disable IPv6 if not needed (uncomment if needed)
# net.ipv6.conf.all.disable_ipv6 = 1
# net.ipv6.conf.default.disable_ipv6 = 1

# Protect against time-wait assassination
net.ipv4.tcp_rfc1337 = 1

# Increase system file descriptor limit
fs.file-max = 65535

# Increase max TCP connections
net.core.somaxconn = 65535
net.ipv4.tcp_max_tw_buckets = 1440000

# Enable ExecShield (if available)
# kernel.exec-shield = 1
kernel.randomize_va_space = 2
EOF

sysctl -p /etc/sysctl.d/99-security.conf

# =============================================================================
# 9. Audit Logging
# =============================================================================
log_info "Configuring audit logging..."

cat > /etc/audit/rules.d/hardening.rules << EOF
# Delete all existing rules
-D

# Buffer Size
-b 8192

# Failure Mode (1=silent, 2=printk)
-f 1

# Audit sudo usage
-w /etc/sudoers -p wa -k sudoers
-w /etc/sudoers.d/ -p wa -k sudoers

# Audit SSH config changes
-w /etc/ssh/sshd_config -p wa -k sshd
-w /etc/ssh/sshd_config.d/ -p wa -k sshd

# Audit user/group changes
-w /etc/passwd -p wa -k identity
-w /etc/group -p wa -k identity
-w /etc/shadow -p wa -k identity

# Audit Docker config
-w /etc/docker/ -p wa -k docker
-w /var/lib/docker/ -p wa -k docker

# Audit network config
-w /etc/network/ -p wa -k network
-w /etc/hosts -p wa -k hosts

# Audit cron jobs
-w /etc/crontab -p wa -k cron
-w /etc/cron.d/ -p wa -k cron
-w /var/spool/cron/ -p wa -k cron
EOF

systemctl enable auditd
systemctl restart auditd

# =============================================================================
# 10. Docker Installation (if not already installed)
# =============================================================================
if ! command -v docker &> /dev/null; then
    log_info "Installing Docker..."
    curl -fsSL https://get.docker.com | sh
fi

# Add deploy user to docker group (whether Docker was just installed or already existed)
if getent group docker > /dev/null 2>&1; then
    usermod -aG docker $DEPLOY_USER
    log_info "Added $DEPLOY_USER to docker group"
else
    log_warn "Docker group not found, skipping group assignment"
fi

# Configure Docker daemon for security
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
    "no-new-privileges": true,
    "storage-driver": "overlay2"
}
EOF

systemctl restart docker

# =============================================================================
# 11. Set Secure File Permissions
# =============================================================================
log_info "Setting secure file permissions..."

chmod 700 /root
chmod 600 /etc/ssh/sshd_config
chmod 700 /etc/ssh/sshd_config.d
chmod 600 /etc/crontab
chmod 700 /etc/cron.d /etc/cron.daily /etc/cron.hourly /etc/cron.monthly /etc/cron.weekly

# =============================================================================
# 12. Create Application Directory
# =============================================================================
log_info "Creating application directory..."

APP_DIR="/var/www/viralclipai-backend"
mkdir -p $APP_DIR
chown $DEPLOY_USER:$DEPLOY_USER $APP_DIR

# =============================================================================
# Done
# =============================================================================
log_info "=========================================="
log_info "Server hardening completed!"
log_info "=========================================="
log_info ""
log_info "IMPORTANT: Before closing this session:"
log_info "1. Open a NEW terminal and test SSH as $DEPLOY_USER:"
log_info "   ssh -p $SSH_PORT $DEPLOY_USER@<server-ip>"
log_info ""
log_info "2. Once confirmed working, restart SSH:"
log_info "   sudo systemctl restart sshd"
log_info ""
log_info "3. Configure SSL with certbot (after setting up nginx):"
log_info "   sudo certbot --nginx -d yourdomain.com"
log_info ""
log_info "Services configured:"
log_info "  - UFW firewall (ports: $SSH_PORT, 80, 443)"
log_info "  - fail2ban (SSH, nginx protection)"
log_info "  - unattended-upgrades (auto security updates)"
log_info "  - auditd (security logging)"
log_info "=========================================="
