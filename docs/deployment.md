# Production Deployment Guide

Distributed architecture: **API droplet** + **Worker droplet** + **Managed Valkey (or Local Redis)** + **Vercel frontend**

---

## Quick Setup

### 1. Vercel Frontend
Set in Vercel Dashboard → Environment Variables:
```
NEXT_PUBLIC_API_BASE_URL = https://api.viralclipai.io
```

### 2. API Server (api.viralclipai.io)

**Initial Setup (Run once on fresh OS):**
```bash
# Upload and run setup script
scp deploy/setup-server.sh ubuntu@<api-ip>:~
ssh ubuntu@<api-ip>
chmod +x setup-server.sh && sudo ./setup-server.sh && sudo reboot
```
*Note: This generates a Deploy Key. Add it to GitHub Repo -> Settings -> Deploy Keys.*

**Application Deployment:**
```bash
ssh deploy@<api-ip>
git clone git@github.com:vmatresu/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

# Apply generated secrets & Configure Application
cp ~/.env.generated .env
sudo ./deploy/provision.sh \
    --role api \
    --redis \
    --worker-ip <worker-ip> \
    --domain api.viralclipai.io \
    --email admin@viralclipai.io
```

### 3. Worker Server

**Initial Setup (Run once on fresh OS):**
```bash
# Upload and run setup script
scp deploy/setup-server.sh ubuntu@<worker-ip>:~
ssh ubuntu@<worker-ip>
chmod +x setup-server.sh && sudo ./setup-server.sh --worker && sudo reboot
```
*Note: Add the generated Deploy Key to GitHub.*

**Application Deployment:**
```bash
ssh deploy@<worker-ip>
git clone git@github.com:vmatresu/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

# Apply secrets & point to API's Redis
cp ~/.env.generated .env
# Edit .env: REDIS_URL=redis://:<password>@<api-ip>:6379

# Configure Application
sudo ./deploy/provision.sh --role worker
```

---

## Managed Valkey Connection (Optional)

If using DigitalOcean Managed Valkey instead of local Redis:
From DO Dashboard → Databases → Your Valkey:
```
VALKEY_URL=rediss://:PASSWORD@db-valkey-xxx.c.db.ondigitalocean.com:25061
```
Use `rediss://` (with s) for TLS.

---

## GitHub Secrets

| Secret | Value |
|--------|-------|
| `API_HOST` | API server IP |
| `WORKER_HOST` | Worker server IP |
| `DO_USER` | `deploy` |
| `DO_PORT` | `22` |
| `DO_SSH_KEY` | Private SSH key (Add pub key to authorized_keys) |

---

## Verification

```bash
curl https://api.viralclipai.io/health  # Should return OK
```

---

## Files Reference

| File | Purpose |
|------|---------|
| `deploy/setup-server.sh` | Initial OS hardening & user setup |
| `deploy/provision.sh` | App config (Nginx, Systemd, SSL, Redis) |
| `deploy/docker-compose.api.yml` | API service definition |
| `deploy/docker-compose.worker.yml` | Worker service definition |
| `deploy/docker-compose.redis.yml` | Optional Redis service |
| `.github/workflows/deploy-api.yml` | API auto-deploy |
| `.github/workflows/deploy-worker.yml` | Worker auto-deploy |
