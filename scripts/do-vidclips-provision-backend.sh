#!/usr/bin/env bash
set -euo pipefail

# Remote provisioning script for Viral Clip AI backend/frontend on a fresh
# DigitalOcean Droplet (Ubuntu 24.04 preferred).
#
# Responsibilities:
# - apt update/upgrade
# - Install Docker Engine + docker compose plugin from official repo
# - Optionally install basic tools (git, ufw)
# - Prepare application directory for GitHub Actions-based deployments
#
# This script is intended to be executed via SSH from the companion
# do-vidclips-create-droplet.sh, e.g.:
#   ssh root@IP 'bash -s' < scripts/do-vidclips-provision-backend.sh

###############################
# Configuration
###############################

: "${APP_DIR:=/var/www/vidclips-gemini}"
: "${CREATE_APP_DIR:=1}"

# Whether to configure a simple UFW firewall (allow SSH/HTTP/HTTPS only)
: "${CONFIGURE_UFW:=1}"

log() {
  echo "[do-vidclips-provision] $*" >&2
}

run_apt_update_upgrade() {
  log "Running apt update/upgrade..."
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -y
  apt-get upgrade -y
}

install_base_packages() {
  log "Installing base packages (ca-certificates, curl, git, ufw)..."
  apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    ufw
}

install_docker() {
  if command -v docker >/dev/null 2>&1; then
    log "Docker already installed, skipping."
    return 0
  fi

  log "Installing Docker Engine and docker compose plugin from official repo..."

  install -m 0755 -d /etc/apt/keyrings
  if ! [ -f /etc/apt/keyrings/docker.gpg ]; then
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | \
      gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    chmod a+r /etc/apt/keyrings/docker.gpg
  fi

  local codename
  codename="$(. /etc/os-release && echo "$VERSION_CODENAME")"

  echo \
    "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
    https://download.docker.com/linux/ubuntu $codename stable" \
    > /etc/apt/sources.list.d/docker.list

  apt-get update -y
  apt-get install -y --no-install-recommends \
    docker-ce \
    docker-ce-cli \
    containerd.io \
    docker-buildx-plugin \
    docker-compose-plugin

  systemctl enable docker
  systemctl start docker

  log "Docker installation complete. Version: $(docker --version)"
}

configure_ufw() {
  if ! command -v ufw >/dev/null 2>&1; then
    log "ufw not installed; skipping firewall configuration."
    return 0
  fi

  log "Configuring UFW firewall (allow SSH, HTTP, HTTPS)..."

  ufw allow OpenSSH || true
  ufw allow 80/tcp || true
  ufw allow 443/tcp || true

  # Uncomment if you want direct access to API/Next.js ports (optional)
  # ufw allow 8000/tcp || true
  # ufw allow 3000/tcp || true

  # Enable UFW if not already enabled
  if ufw status | grep -q "Status: inactive"; then
    echo "y" | ufw enable || true
  fi

  ufw status verbose || true
}

prepare_app_dir() {
  if [[ "$CREATE_APP_DIR" -eq 1 ]]; then
    log "Ensuring application directory exists at $APP_DIR..."
    mkdir -p "$APP_DIR"
  fi

  log "Current contents of $APP_DIR:"
  ls -la "$APP_DIR" || true
}

###############################
# Main
###############################

log "Starting provisioning on host $(hostname)"

run_apt_update_upgrade
install_base_packages
install_docker

if [[ "$CONFIGURE_UFW" -eq 1 ]]; then
  configure_ufw
fi

prepare_app_dir

log "Provisioning completed successfully. You can now deploy via GitHub Actions to $APP_DIR using docker-compose."
