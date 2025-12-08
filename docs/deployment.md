# Production Deployment Guide

This document describes how to run Viral Clip AI in development and production with security hardening.

---

## Quick Start (Production)

```bash
# 1. On DROPLET - Run server hardening
sudo ./deploy/server-hardening.sh deploy

# 2. On DROPLET - Setup SSL
sudo ./deploy/certbot-setup.sh api.yourdomain.com admin@yourdomain.com

# 3. On DROPLET - Configure environment
cp .env.example .env  # Edit with production values

# 4. On DROPLET - Start services
docker compose up -d
```

---

## Local Development

See `DOCKER_SETUP.md` for a detailed quickstart. In summary:

```bash
# 1. Setup environment files
cp .env.dev.example .env.dev
cp web/.env.local.example web/.env.local

# 2. Run development stack
docker-compose -f docker-compose.dev.yml up --build
```

- Backend API: `http://localhost:8000`
- Frontend: `http://localhost:3000`

---

## Production Server Setup

### 1. Initial Droplet Setup

```bash
# SSH as root
ssh root@your-droplet-ip

# Clone repository
git clone https://github.com/yourusername/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

# Run hardening script (creates 'deploy' user)
chmod +x deploy/server-hardening.sh
./deploy/server-hardening.sh deploy 22

# IMPORTANT: Test SSH in NEW terminal before closing!
# ssh deploy@your-droplet-ip
```

### 2. SSL Configuration

```bash
# Copy nginx template and setup SSL
sudo cp deploy/nginx/nginx.conf /etc/nginx/nginx.conf.template
sudo chmod +x deploy/certbot-setup.sh
sudo ./deploy/certbot-setup.sh api.viralclipai.com
```

### 3. Environment Configuration

Create `.env` on server with:

```bash
# Firebase (from Firebase Console)
FIREBASE_PROJECT_ID=your-project-id
FIREBASE_PRIVATE_KEY="-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n"
FIREBASE_CLIENT_EMAIL=firebase-adminsdk@your-project.iam.gserviceaccount.com

# R2 Storage
R2_ENDPOINT_URL=https://account-id.r2.cloudflarestorage.com
R2_ACCESS_KEY_ID=your-key
R2_SECRET_ACCESS_KEY=your-secret
R2_BUCKET_NAME=your-bucket

# Security secrets (generate with: openssl rand -base64 64)
JWT_SECRET=your-random-string
REDIS_PASSWORD=your-redis-password
```

---

## GitHub Actions Deployment

Configure these GitHub secrets (Settings → Secrets):

| Secret | Value |
|--------|-------|
| `DO_HOST` | Droplet IP |
| `DO_USER` | `deploy` |
| `DO_PORT` | SSH port (default: 22) |
| `DO_SSH_KEY` | Private SSH key |

Pushes to `main` auto-deploy via `.github/workflows/deploy.yml`.

---

## Security Features

### Server Hardening (deploy/server-hardening.sh)

- SSH: Root login disabled, key-only auth, strong ciphers
- Firewall: UFW with minimal ports (22, 80, 443)
- Intrusion detection: fail2ban with auto-ban
- Auto-updates: unattended-upgrades for security patches
- Audit logging: auditd for security events
- Kernel: sysctl hardening (SYN flood protection, etc.)

### Docker Security (docker-compose.yml)

- Non-root containers
- `no-new-privileges` enabled
- All capabilities dropped
- Read-only filesystems where possible
- Redis password authentication
- No external Redis port binding

### CI/CD Security (.github/workflows/)

- SHA-pinned GitHub Actions
- Trivy container vulnerability scanning
- cargo-audit for Rust dependencies
- npm-audit for web dependencies
- SBOM generation
- Secret detection with TruffleHog

---

## Monitoring

```bash
# Service health
docker compose ps
curl http://localhost:8000/health

# Security status
sudo fail2ban-client status sshd
sudo ufw status

# Logs
docker compose logs -f api worker
```

---

## Backup & Recovery

```bash
# Backup Redis
docker compose exec redis redis-cli -a $REDIS_PASSWORD BGSAVE
docker cp vclip-redis:/data/dump.rdb ./backups/

# Restore
docker compose down
docker cp ./backups/dump.rdb vclip-redis:/data/
docker compose up -d
```

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| API not responding | `docker compose logs api --tail=100` |
| Redis auth failed | Check `REDIS_PASSWORD` matches in API, Worker, and Redis |
| SSL issues | `sudo certbot renew --dry-run` |
| SSH locked out | Access via DO console, check `/etc/ssh/sshd_config.d/` |

For storage configuration, see `docs/storage-and-media.md`.
