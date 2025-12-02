#!/usr/bin/env bash
set -euo pipefail

# DigitalOcean Droplet creation + optional provisioning for the Viral Clip AI stack.
#
# Features / best practices:
# - Strict bash options (fail fast, no unbound vars, pipefail)
# - Centralized config via variables and env overrides
# - Idempotent droplet creation (refuses if name already exists)
# - Optional auto-provisioning of the droplet over SSH
#
# Requirements:
# - doctl installed and authenticated (https://docs.digitalocean.com/reference/doctl)
# - A DigitalOcean SSH key set up; see DO_SSH_KEY_REF below.

###############################
# Configuration
###############################

# doctl binary
: "${DOCTL_BIN:=doctl}"

# Droplet settings (override via env vars as needed)
: "${DO_REGION:=nyc1}"
: "${DO_IMAGE:=ubuntu-24-04-x64}"
: "${DO_SIZE:=s-1vcpu-1gb}"

# Default SSH key ID for this account (from `doctl compute ssh-key list`).
# The example uses the same ID as your reference script; override if different.
: "${DO_SSH_KEY_REF:=51577102}"

# Unique droplet name for this project
: "${DO_DROPLET_NAME:=viralvideoai-nyc1-01}"

# Tags for grouping, firewalls, etc.
: "${DO_TAGS:=viralvideoai,fastapi,nextjs,production}"

# If set to 1, script will immediately provision the server over SSH
: "${AUTO_PROVISION:=1}"

# SSH user for initial login (DigitalOcean defaults to root for Ubuntu images)
: "${SSH_USER:=root}"

# Optional path to the SSH private key used to connect to the Droplet. If empty,
# the default SSH configuration / agent will be used.
: "${SSH_KEY_PATH:=}"

# Path to the remote provisioning script that will be executed on the Droplet
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVISION_SCRIPT_LOCAL_PATH="${SCRIPT_DIR}/do-viralclipai-provision-backend.sh"

###############################
# Helper functions
###############################

log() {
  echo "[do-viralclipai-create-droplet] $*" >&2
}

check_prerequisites() {
  if ! command -v "$DOCTL_BIN" >/dev/null 2>&1; then
    log "ERROR: doctl is not installed or not in PATH."
    log "Install from https://docs.digitalocean.com/reference/doctl/how-to/install/ and run 'doctl auth init'."
    exit 1
  fi

  if [[ ! -x "$DOCTL_BIN" && "$DOCTL_BIN" != "doctl" ]]; then
    log "ERROR: DOCTL_BIN points to '$DOCTL_BIN' which is not executable."
    exit 1
  fi

  if [[ "$AUTO_PROVISION" -eq 1 && ! -f "$PROVISION_SCRIPT_LOCAL_PATH" ]]; then
    log "ERROR: Provisioning enabled but script not found at $PROVISION_SCRIPT_LOCAL_PATH"
    log "Create that script or set AUTO_PROVISION=0."
    exit 1
  fi
}

ensure_unique_droplet_name() {
  if "$DOCTL_BIN" compute droplet list --format Name --no-header | grep -Fxq "$DO_DROPLET_NAME"; then
    log "ERROR: A droplet named '$DO_DROPLET_NAME' already exists. Pick a different DO_DROPLET_NAME."
    exit 1
  fi
}

create_droplet() {
  log "Creating droplet '$DO_DROPLET_NAME' in region '$DO_REGION' (image=$DO_IMAGE size=$DO_SIZE)..."

  local json
  json=$("$DOCTL_BIN" compute droplet create "$DO_DROPLET_NAME" \
    --region "$DO_REGION" \
    --image "$DO_IMAGE" \
    --size "$DO_SIZE" \
    --ssh-keys "$DO_SSH_KEY_REF" \
    --tag-names "$DO_TAGS" \
    --enable-backups=false \
    --enable-ipv6=false \
    --wait \
    -o json)

  echo "$json"
}

check_api_error() {
  local json="$1"
  if echo "$json" | grep -q '"errors"'; then
    log "ERROR: DigitalOcean API reported an error while creating the droplet:"
    log "$json"
    exit 1
  fi
}

extract_ip_from_json() {
  local json="$1"
  if command -v jq >/dev/null 2>&1; then
    echo "$json" | jq -r '.[0].networks.v4[] | select(.type == "public") | .ip_address' | head -n 1
    return 0
  fi
  echo "$json" | sed -n 's/.*"ip_address"[[:space:]]*:[[:space:]]*"\([0-9.]*\)".*/\1/p' | head -n 1
}

wait_for_ssh() {
  local host="$1" max_attempts=30 delay=5
  log "Waiting for SSH to become available on $host..."
  for ((i=1; i<=max_attempts; i++)); do
    local ssh_cmd=(ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=accept-new)
    if [[ -n "$SSH_KEY_PATH" ]]; then
      ssh_cmd+=(-i "$SSH_KEY_PATH")
    fi
    ssh_cmd+=("${SSH_USER}@${host}" true)
    if "${ssh_cmd[@]}" 2>/dev/null; then
      log "SSH is available on $host."
      return 0
    fi
    log "SSH not ready yet (attempt $i/$max_attempts). Retrying in ${delay}s..."
    sleep "$delay"
  done
  log "ERROR: SSH did not become ready on $host in time."
  return 1
}

provision_remote() {
  local host="$1"
  log "Provisioning remote host $host using $PROVISION_SCRIPT_LOCAL_PATH..."

  local ssh_cmd=(ssh -o StrictHostKeyChecking=accept-new)
  if [[ -n "$SSH_KEY_PATH" ]]; then
    ssh_cmd+=(-i "$SSH_KEY_PATH")
  fi
  ssh_cmd+=("${SSH_USER}@${host}" "bash -s")
  "${ssh_cmd[@]}" < "$PROVISION_SCRIPT_LOCAL_PATH"
}

###############################
# Main
###############################

check_prerequisites
ensure_unique_droplet_name

json_output="$(create_droplet)"

check_api_error "$json_output"

log "Droplet created. Raw JSON from doctl:"
log "$json_output"

DROPLET_IP="$(extract_ip_from_json "$json_output")"

if [[ -z "$DROPLET_IP" || "$DROPLET_IP" == "null" ]]; then
  log "Failed to parse IP from create response; querying doctl for droplet IP..."
  fallback_json=$("$DOCTL_BIN" compute droplet get "$DO_DROPLET_NAME" -o json)
  DROPLET_IP="$(extract_ip_from_json "$fallback_json")"
fi

if [[ -z "$DROPLET_IP" || "$DROPLET_IP" == "null" ]]; then
  log "ERROR: Could not determine droplet public IP."
  log "You can inspect the droplet manually with: doctl compute droplet get $DO_DROPLET_NAME -o json"
  exit 1
fi

log "Droplet '$DO_DROPLET_NAME' is up with IP: $DROPLET_IP"

if [[ "$AUTO_PROVISION" -eq 1 ]]; then
  if [[ -z "$SSH_KEY_PATH" ]]; then
    log "NOTE: AUTO_PROVISION is enabled but SSH_KEY_PATH is empty. Using default SSH configuration."
  fi
  wait_for_ssh "$DROPLET_IP"
  provision_remote "$DROPLET_IP"
  log "Provisioning complete."
fi

cat <<EOF

=== Droplet Created ===
Name:        $DO_DROPLET_NAME
Region:      $DO_REGION
IP Address:  $DROPLET_IP
SSH user:    $SSH_USER

To connect manually:
  ssh ${SSH_USER}@${DROPLET_IP}

To re-run provisioning only (if AUTO_PROVISION=0 or you changed the script), run:
  ssh ${SSH_USER}@${DROPLET_IP} 'bash -s' < "${PROVISION_SCRIPT_LOCAL_PATH}"

EOF
