# Production Deployment Guide

Distributed architecture: **API droplet** + **Worker droplet** + **Managed Valkey** + **Vercel frontend**

---

## Quick Setup

### 1. Vercel Frontend
Set in Vercel Dashboard → Environment Variables:
```
NEXT_PUBLIC_API_BASE_URL = https://api.viralclipai.io
```

### 2. API Droplet (api.viralclipai.io)
```bash
ssh root@api-droplet-ip
git clone https://github.com/youruser/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

sudo ./deploy/server-hardening.sh deploy
sudo ./deploy/certbot-setup.sh api.viralclipai.io
cp deploy/.env.api.example .env  # Edit with your values
docker compose -f deploy/docker-compose.api.yml up -d
```

### 3. Worker Droplet
```bash
ssh root@worker-droplet-ip
git clone https://github.com/youruser/viralclipai.git /var/www/viralclipai-backend
cd /var/www/viralclipai-backend

sudo ./deploy/server-hardening.sh deploy
cp deploy/.env.worker.example .env  # Edit with your values
docker compose -f deploy/docker-compose.worker.yml up -d
```

---

## Managed Valkey Connection

From DO Dashboard → Databases → Your Valkey:
```
VALKEY_URL=rediss://:PASSWORD@db-valkey-xxx.c.db.ondigitalocean.com:25061
```
Use `rediss://` (with s) for TLS.

---

## GitHub Secrets

| Secret | Value |
|--------|-------|
| `API_HOST` | API droplet IP |
| `WORKER_HOST` | Worker droplet IP |
| `DO_USER` | `deploy` |
| `DO_PORT` | `22` |
| `DO_SSH_KEY` | Private SSH key |

---

## Verification

```bash
curl https://api.viralclipai.io/health  # Should return OK
```

---

## Files Reference

| File | Purpose |
|------|---------|
| `deploy/docker-compose.api.yml` | API service |
| `deploy/docker-compose.worker.yml` | Worker service |
| `deploy/.env.api.example` | API env template |
| `deploy/.env.worker.example` | Worker env template |
| `.github/workflows/deploy-api.yml` | API auto-deploy |
| `.github/workflows/deploy-worker.yml` | Worker auto-deploy |
