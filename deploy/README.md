# ViralClipAI - Deployment Guide

> **Repository:** https://github.com/vmatresu/viralclipai
> **Frontend:** https://www.viralclipai.io (Vercel)
> **API:** https://api.viralclipai.io (DigitalOcean/OVH)

Production deployment guide for ViralClipAI backend with separate API and Worker servers.

---

## Architecture

```
┌─────────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│   Vercel            │     │  API Server          │     │  Worker Server  │
│   (Frontend)        │────▶│  api.viralclipai.io  │     │  (Private IP)   │
│ www.viralclipai.io  │     │  + Redis (Optional)  │◀─── │                 │
└─────────────────────┘     └──────────┬───────────┘     └────────┬────────┘
                                       │                          │
                                       │    ┌─────────────────┐   │
                                       └───▶│ Managed Valkey  │◀──┘
                                            │ (Alternative)   │
                                            └─────────────────┘
```

---

## Quick Reference

| Service | Compose File | systemd Unit | Container |
|---------|--------------|--------------|-----------|
| API | `deploy/docker-compose.api.yml` | `viralclip-api.service` | `vclip-api` |
| Worker | `deploy/docker-compose.worker.yml` | `viralclip-worker.service` | `vclip-worker` |
| Redis | `deploy/docker-compose.redis.yml` | (Merged into API) | `vclip-redis` |

---

## Prerequisites

- **2x Ubuntu 24.04 Servers** (API & Worker)
- **Cloudflare R2** bucket
- **Firebase** project with Firestore
- **GitHub** repository access

---

## 1. Initial Server Setup (One-Time)

Run this on a fresh Ubuntu installation to harden the server, install Docker, and set up the `deploy` user.

**API Server:**
```bash
# Upload and run setup script
scp deploy/setup-server.sh ubuntu@<api-ip>:~
ssh ubuntu@<api-ip>
chmod +x setup-server.sh && sudo ./setup-server.sh && sudo reboot
```

**Worker Server:**
```bash
# Upload and run setup script
scp deploy/setup-server.sh ubuntu@<worker-ip>:~
ssh ubuntu@<worker-ip>
chmod +x setup-server.sh && sudo ./setup-server.sh --worker && sudo reboot
```

**After Setup:**
1.  **Deploy Keys:** The script will output a public SSH key. Add this to your **GitHub Repo -> Settings -> Deploy Keys**.
2.  **GitHub Actions:** Add your GitHub Actions SSH public key to `/home/deploy/.ssh/authorized_keys` as instructed in the script output.

---

## 2. Application Deployment

Log in as the `deploy` user to configure the application.

### API Server
```bash
ssh deploy@<api-ip>
git clone git@github.com:vmatresu/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

# Apply secrets & Edit config
cp ~/.env.generated .env
nano .env

# Provision Application (Nginx + Redis + SSL + Systemd)
sudo ./deploy/provision.sh \
    --role api \
    --redis \
    --worker-ip <worker-ip> \
    --domain api.viralclipai.io \
    --email admin@viralclipai.io
```

### Worker Server
```bash
ssh deploy@<worker-ip>
git clone git@github.com:vmatresu/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

# Apply secrets & Edit config
cp ~/.env.generated .env
nano .env  # Set REDIS_URL to point to API server

# Provision Application (Systemd)
sudo ./deploy/provision.sh --role worker
```

---

## Service Management

The `provision.sh` script installs a systemd service that manages Docker Compose.

```bash
# Check Status
sudo systemctl status viralclip-api.service
sudo systemctl status viralclip-worker.service

# View Logs
journalctl -u viralclip-api.service -f
journalctl -u viralclip-worker.service -f

# Restart Service
sudo systemctl restart viralclip-api.service

# Manual Docker (if needed)
docker compose -f deploy/docker-compose.api.yml logs -f
```

---

## Deployments

### Automatic (GitHub Actions)
Pushing to `main` triggers the workflow defined in `.github/workflows/`.

### Manual Deployment
```bash
cd /var/www/viralclipai-backend
git pull origin main
sudo systemctl restart viralclip-api.service
```

---

## File Structure

```
deploy/
├── README.md                      # This file
├── setup-server.sh                # Initial OS hardening & user setup
├── provision.sh                   # App configuration (Nginx, Systemd, Redis, SSL)
├── docker-compose.api.yml         # API service definition
├── docker-compose.worker.yml      # Worker service definition
├── docker-compose.redis.yml       # Modular Redis service definition
├── nginx/                         # Nginx config templates
├── redis/                         # Redis config templates
└── systemd/                       # Systemd unit templates
```

---

## Security Checklist

- [x] SSH key-only authentication
- [x] Root login disabled
- [x] UFW firewall active (API ports 80/443/6379, Worker port 22)
- [x] fail2ban running
- [x] Automatic security updates enabled
- [x] `.env` file permissions restricted
- [x] SSL certificates via Certbot
- [x] Systemd service manages containers

```