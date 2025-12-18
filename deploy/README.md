# ViralClipAI - Deployment Guide

> **Repository:** https://github.com/vmatresu/viralclipai
> **Frontend:** https://www.viralclipai.io (Vercel)
> **API:** https://api.viralclipai.io (DigitalOcean)

Production deployment guide for ViralClipAI backend on DigitalOcean with separate API and Worker droplets.

---

## Architecture

```
┌─────────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│   Vercel            │     │  DigitalOcean        │     │  DigitalOcean   │
│   (Frontend)        │────▶│  API Droplet         │     │  Worker Droplet │
│ www.viralclipai.io  │     │  api.viralclipai.io  │     │  (no public URL)│
└─────────────────────┘     └──────────┬───────────┘     └────────┬────────┘
                                       │                          │
                                       │    ┌─────────────────┐   │
                                       └───▶│ Managed Valkey  │◀──┘
                                            │ (Redis Queue)   │
                                            └─────────────────┘
```

---

## Quick Reference

| Service | Compose File | systemd Unit | Container |
|---------|--------------|--------------|-----------|
| API | `deploy/docker-compose.api.yml` | `viralclip-api.service` | `vclip-api` |
| Worker | `deploy/docker-compose.worker.yml` | `viralclip-worker.service` | `vclip-worker` |

---

## Prerequisites

- **2x DigitalOcean Droplets** (Ubuntu 24.04)
  - API: 2GB+ RAM
  - Worker: 8GB+ RAM (video processing)
- **DigitalOcean Managed Valkey** database
- **Cloudflare R2** bucket
- **Firebase** project with Firestore
- **GitHub** repository access

---

## Initial Server Setup

### 1. Create Droplets

**API Droplet:**
- Image: Ubuntu 24.04
- Plan: Basic, 2GB RAM ($12/mo)
- Region: NYC1
- Hostname: `viralclipai-api-nyc1-01`

**Worker Droplet:**
- Image: Ubuntu 24.04
- Plan: Basic, 8GB RAM ($48/mo)
- Region: NYC1
- Hostname: `viralclipai-worker-nyc1-01`

### 2. Run Setup Script (Unified)

This single script handles hardening (UFW, Fail2ban), installs Docker, creates the deploy user, and **auto-generates secure secrets** (Redis password, JWT secret) in a new `.env` file.

```bash
# On BOTH servers (run as root):
ssh root@<server-ip>
curl -O https://raw.githubusercontent.com/vmatresu/viralclipai/main/deploy/setup-server.sh
chmod +x setup-server.sh
./setup-server.sh
```

**After running:**
1.  **Edit the `.env` file**: The script creates `/var/www/viralclipai-backend/.env`. You **must** edit it to fill in your Cloudflare R2 keys and Firebase Project ID.
    ```bash
    nano /var/www/viralclipai-backend/.env
    ```
2.  **Test SSH**: Ensure you can log in as `deploy` before closing your root session.
    ```bash
    ssh deploy@<server-ip>
    ```

### 3. Setup GitHub Deploy Keys

On each server (as `deploy` user):

```bash
ssh-keygen -t ed25519 -C "deploy@viralclipai" -f ~/.ssh/github_deploy -N ""
cat ~/.ssh/github_deploy.pub  # Add to GitHub repo deploy keys

# Configure SSH
cat >> ~/.ssh/config << 'EOF'
Host github.com
    HostName github.com
    User git
    IdentityFile ~/.ssh/github_deploy
    IdentitiesOnly yes
EOF
chmod 600 ~/.ssh/config

# Test connection
ssh -T git@github.com
```

### 4. Clone Repository

```bash
cd /var/www
git clone git@github.com:vmatresu/viralclipai.git viralclipai-backend
cd viralclipai-backend
```

### 5. Configure Environment

Create `.env` file (see `.env.example` for template):

```bash
cp .env.example .env
chmod 600 .env
# Edit with your values
```

**Required files:**
- `.env` - Environment configuration
- `firebase-credentials.json` - Firebase service account
- `youtube-cookies.txt` - YouTube authentication (Worker only)

---

## Service Management

### Install systemd Services (Recommended)

The systemd service ensures containers **automatically restart after reboot**.

```bash
# Install API service
cd /var/www/viralclipai-backend
sudo ./deploy/install-systemd.sh api

# Install Worker service
sudo ./deploy/install-systemd.sh worker
```

### Manage Services

```bash
# Status
sudo systemctl status viralclip-api.service
sudo systemctl status viralclip-worker.service

# Start/Stop/Restart
sudo systemctl start viralclip-api.service
sudo systemctl stop viralclip-api.service
sudo systemctl restart viralclip-api.service

# Reload (recreate containers without stopping service)
sudo systemctl reload viralclip-api.service

# View logs
journalctl -u viralclip-api.service -f
journalctl -u viralclip-worker.service -f

# Uninstall
sudo ./deploy/install-systemd.sh api uninstall
```

### Manual Docker Commands

For one-off operations without systemd:

```bash
cd /var/www/viralclipai-backend

# Start
docker compose --env-file .env -f deploy/docker-compose.api.yml up -d

# Stop
docker compose --env-file .env -f deploy/docker-compose.api.yml down

# Rebuild
docker compose --env-file .env -f deploy/docker-compose.api.yml up -d --build

# Logs
docker logs vclip-api -f --tail 100
docker logs vclip-worker -f --tail 100
```

---

## Deployments

### Automatic (GitHub Actions)

Deployments trigger automatically on push to `main` when relevant files change:
- **API**: `backend/**`, `Dockerfile`, `deploy/docker-compose.api.yml`
- **Worker**: `backend/**`, `Dockerfile`, `deploy/docker-compose.worker.yml`

### Manual Deployment

```bash
cd /var/www/viralclipai-backend
git pull origin main

# Using systemd (recommended)
sudo systemctl reload viralclip-api.service

# Or using docker compose directly
docker compose --env-file .env -f deploy/docker-compose.api.yml up -d --build
```

---

## Nginx Configuration (API Only)

```bash
sudo tee /etc/nginx/sites-available/viralclipai << 'EOF'
server {
    listen 80;
    server_name api.viralclipai.io;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;
        proxy_connect_timeout 75s;
    }
}
EOF

sudo ln -sf /etc/nginx/sites-available/viralclipai /etc/nginx/sites-enabled/
sudo rm -f /etc/nginx/sites-enabled/default
sudo nginx -t && sudo systemctl reload nginx

# Setup SSL
sudo certbot --nginx -d api.viralclipai.io
```

---

## Troubleshooting

### Container not starting after reboot

**Symptom:** `docker ps` shows no containers after server reboot.

**Cause:** Docker's restart policy only works if containers exist. If containers were removed (e.g., via `docker system prune`), they won't auto-start.

**Solution:** Install the systemd service which creates containers on boot:
```bash
sudo ./deploy/install-systemd.sh api
```

### Service fails to start

```bash
# Check systemd logs
journalctl -u viralclip-api.service -n 50 --no-pager

# Check Docker logs
docker logs vclip-api --tail 50

# Verify prerequisites
test -f /var/www/viralclipai-backend/.env && echo "OK: .env exists"
test -f /var/www/viralclipai-backend/firebase-credentials.json && echo "OK: firebase creds exist"
```

### Permission denied

```bash
# Ensure deploy user is in docker group
groups deploy  # Should show: deploy docker

# If not:
sudo usermod -aG docker deploy
# Logout and login again
```

### Port already in use

```bash
# Check what's using port 8000
sudo ss -tlnp | grep :8000
sudo lsof -i :8000
```

### Safe pruning (avoid deleting active containers)

```bash
# NEVER use on production: docker system prune -a

# Safe alternatives:
docker container prune --filter "until=24h" -f
docker image prune --filter "until=168h" -f
```

---

## File Structure

```
deploy/
├── README.md                      # This file
├── install-systemd.sh             # Unified service installer
├── systemd/
│   ├── viralclip-api.service      # API systemd unit
│   └── viralclip-worker.service   # Worker systemd unit
├── docker-compose.api.yml         # API Docker Compose
├── docker-compose.worker.yml      # Worker Docker Compose
├── server-hardening.sh            # API server setup
├── server-hardening-worker.sh     # Worker server setup
└── certbot-setup.sh               # SSL certificate setup
```

---

## Security Checklist

- [ ] SSH key-only authentication
- [ ] Root login disabled
- [ ] UFW firewall active
- [ ] fail2ban running
- [ ] Automatic security updates enabled
- [ ] Valkey trusted sources configured
- [ ] `.env` file has 600 permissions
- [ ] SSL certificates installed (API)
- [ ] systemd service uses security hardening

---

## GitHub Actions Secrets

| Secret | Description |
|--------|-------------|
| `API_HOST` | API droplet IP |
| `WORKER_HOST` | Worker droplet IP |
| `DO_USER` | SSH user (`deploy`) |
| `DO_PORT` | SSH port (`22`) |
| `DO_SSH_KEY` | Private SSH key |
