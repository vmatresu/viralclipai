# IPv6 Rotation Implementation Walkthrough

## Summary

Implemented IPv6 address rotation for the ViralClip worker container to avoid YouTube rate limiting. When configured, yt-dlp and youtubei.js will randomly select from 1000+ IPv6 addresses assigned to the container for each request.

## Changes Made

### 1. Server Provisioning Scripts

#### assign-ipv6.sh (NEW)

Script to assign multiple IPv6 addresses to Docker containers using nsenter:

```bash
# Assigns 1000 IPv6 addresses to vclip-worker container
sudo /usr/local/bin/assign-ipv6.sh --container vclip-worker --count 1000
```

Features:

- Reads configuration from `/etc/viralclip/ipv6.conf`
- Uses batch execution via nsenter for efficiency
- Skips already-assigned addresses gracefully

#### setup-server.sh (MODIFIED)

Added `step_ipv6()` function and CLI arguments:

```bash
# Enable IPv6 rotation when setting up worker server
sudo ./deploy/setup-server.sh --worker --ipv6-subnet "2001:41d0:xxx::/64"
```

Configures:

- IPv6 forwarding via sysctl
- ndppd (NDP Proxy Daemon) for neighbor discovery
- Systemd service to auto-assign IPs after Docker starts

### 2. Docker Configuration

#### docker-compose.worker.yml (MODIFIED)

```yaml
# =============================================================================
# ViralClip AI - Worker Service (Standalone)
# =============================================================================
# For deployment on dedicated Worker droplet with external managed Valkey
# Usage: docker compose -f docker-compose.worker.yml up -d
#
# IPv6 Rotation:
#   Set DOCKER_IPV6_SUBNET in .env to enable IPv6 rotation (e.g., 2001:41d0:xxx::/64)
#   After container starts, run: sudo /usr/local/bin/assign-ipv6.sh
# =============================================================================
services:
  # ... existing service config ...
  # Temp storage for video processing
  - worker-temp:/tmp/videos

  # Network configuration - attach to IPv6-enabled network if available
  networks:
    - viralclip-ipv6

  # Enable IPv6 sysctls in container (required for IPv6 address assignment)
  sysctls:
    - net.ipv6.conf.all.disable_ipv6=0
    - net.ipv6.conf.default.disable_ipv6=0

  healthcheck:
    test: ["CMD", "pgrep", "-x", "vclip-worker"]
    interval: 30s
    timeout: 10s
    retries: 3
    start_period: 40s

# ... existing volumes ...

# =============================================================================
# Networks
# =============================================================================
# IPv6-enabled network for IP rotation to avoid YouTube rate limiting.
# The subnet must match the one assigned by your hosting provider.
# Set DOCKER_IPV6_SUBNET in .env (e.g., 2001:41d0:xxx::/64)
#
# After container starts, IPv6 addresses are assigned via:
#   sudo /usr/local/bin/assign-ipv6.sh
# =============================================================================
networks:
  viralclip-ipv6:
    driver: bridge
    enable_ipv6: true
    ipam:
      config:
        - subnet: 172.20.0.0/16
        - subnet: ${DOCKER_IPV6_SUBNET:-fd00:dead:beef::/48}

volumes:
  worker-temp:
    name: vclip-worker-temp
    driver: local
```

Key changes:

- Added `viralclip-ipv6` network with IPv6 enabled
- Added sysctls to enable IPv6 in container
- Network uses `DOCKER_IPV6_SUBNET` from `.env` (default: `fd00:dead:beef::/48`)

### 3. Application-Level IP Rotation

#### download.rs (MODIFIED)

Added `get_random_ipv6_address()` function for Rust yt-dlp calls:

```rust
pub fn get_random_ipv6_address() -> Option<String> {
    // Reads network interfaces via nix crate
    // Filters to global IPv6 addresses only
    // Returns random selection
}
```

Both `download_video()` and `download_segment()` now use `--source-address` flag:

```rust
// IPv6 rotation: select random source address if available
let ipv6_source = get_random_ipv6_address();
if let Some(ip) = ipv6_source.as_deref() {
    args.push("--source-address");
    args.push(ip);
    info!("Using IPv6 source address for download: {}", ip);
}
```

Dependencies added to `Cargo.toml`:

```toml
nix = { version = "0.30", features = ["net"] }
rand = "0.9"
```

#### youtubei-transcript.mjs (MODIFIED)

Added IPv6 rotation using undici Agent with localAddress:

```javascript
function getRandomIPv6Address() {
  const interfaces = os.networkInterfaces();
  const globalAddresses = [];
  // Filter to global IPv6 only
  return globalAddresses[Math.floor(Math.random() * globalAddresses.length)];
}

function createBoundFetch(localAddress) {
  const agent = new Agent({
    connect: { localAddress },
  });
  return (url, options = {}) =>
    undiciFetch(url, { ...options, dispatcher: agent });
}

// Usage:
const ipv6Address = getRandomIPv6Address();
if (ipv6Address) {
  innertubeOptions.fetch = createBoundFetch(ipv6Address);
}
```

Dependencies added to `package.json`:

```json
{
  "undici": "^7.2.0"
}
```

### 4. Deployment Workflow

#### deploy-worker.yml (MODIFIED)

Added IPv6 assignment step after container deployment:

```bash
# Assign IPv6 addresses for IP rotation (if script exists and IPv6 is configured)
if [ -f /usr/local/bin/assign-ipv6.sh ] && [ -f /etc/viralclip/ipv6.conf ]; then
  echo "Assigning IPv6 addresses..."
  /usr/local/bin/assign-ipv6.sh 2>&1 | tee -a /tmp/worker-deploy.log
fi
```

## Setup Instructions

### 1. Get IPv6 Subnet from Hosting Provider

Contact your hosting provider (OVH, Hetzner, etc.) to get an IPv6 subnet allocation. Example: `2001:41d0:xxx:xxxx::/64`

### 2. Configure Server (One-time)

```bash
# SSH to worker server
ssh deploy@<worker-ip>

# Run setup with IPv6 subnet
sudo ./deploy/setup-server.sh --worker \
  --ipv6-subnet "2001:41d0:xxx:xxxx::/64" \
  --ipv6-interface "eno1"
```

### 3. Add to .env

```bash
# Add to /var/www/viralclipai-backend/.env
DOCKER_IPV6_SUBNET=2001:41d0:xxx:xxxx::/64
```

### 4. Deploy

Push to `main` branch or manually trigger the workflow. The IPv6 addresses will be assigned automatically.

## Verification

### Check IPv6 addresses on container

```bash
docker exec vclip-worker ip -6 addr show eth0 | grep inet6 | wc -l
# Expected: 1000+ addresses
```

### Check yt-dlp uses IPv6

```bash
docker exec vclip-worker yt-dlp --source-address 2001:41d0:xxx:xxxx::64 --get-url https://www.youtube.com/watch?v=dQw4w9WgXcQ
```

### Check logs for IPv6 rotation

```bash
docker logs vclip-worker 2>&1 | grep "IPv6 source address"
```

## Architecture

```
YouTube Request
      ↓
Worker Container
      ↓ get_random_ipv6_address
      ↓
1000+ IPv6 Addresses
      ↓ Random Selection
      ↓
yt-dlp --source-address
      ↓
   YouTube API
      ↑
   Host Server
      ↑
     ndppd
     ↑ NDP Proxy
     ↑
 Docker Network
     ↑
assign-ipv6.sh
     ↑ nsenter
     ↑
Container Network NS
```
