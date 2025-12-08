# ViralClipAI - Distributed Deployment Guide

> **Repository:** https://github.com/vmatresu/viralclipai  
> **Frontend:** https://www.viralclipai.io (Vercel)  
> **API:** https://api.viralclipai.io (DigitalOcean)

This guide covers setting up the ViralClipAI backend on DigitalOcean with separate API and Worker droplets.

## Architecture Overview

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

## Prerequisites

- 2x DigitalOcean Droplets (Ubuntu 24.04)
  - API: 2GB+ RAM recommended
  - Worker: 8GB+ RAM recommended (video processing)
- DigitalOcean Managed Valkey database
- Cloudflare R2 bucket
- Firebase project with Firestore
- GitHub repository access

---

## Step 1: Create DigitalOcean Droplets

### Create Droplets in DO Console:

1. **API Droplet:**
   - Image: Ubuntu 24.04
   - Plan: Basic, 2GB RAM ($12/mo) or higher
   - Region: NYC1 (or your preferred)
   - Add your SSH key
   - Hostname: `viralclipai-api-nyc1-01`

2. **Worker Droplet:**
   - Image: Ubuntu 24.04
   - Plan: Basic, 8GB RAM ($48/mo) - video processing needs RAM
   - Region: NYC1 (same as API)
   - Add your SSH key
   - Hostname: `viralclipai-worker-nyc1-01`

---

## Step 2: Run Server Hardening Scripts

### On the API Server:

```bash
# SSH as root
ssh root@<api-droplet-ip>

# Download and run the API hardening script
curl -O https://raw.githubusercontent.com/vmatresu/viralclipai/main/deploy/server-hardening.sh
chmod +x server-hardening.sh
./server-hardening.sh deploy
```

### On the Worker Server:

```bash
# SSH as root
ssh root@<worker-droplet-ip>

# Download and run the Worker hardening script
curl -O https://raw.githubusercontent.com/vmatresu/viralclipai/main/deploy/server-hardening-worker.sh
chmod +x server-hardening-worker.sh
./server-hardening-worker.sh deploy
```

### After running each script:

1. **Test SSH as deploy user** (in a new terminal):
   ```bash
   ssh deploy@<droplet-ip>
   ```

2. **If it works**, restart SSH from the server:
   ```bash
   sudo systemctl restart ssh
   ```

> ⚠️ **Important:** Don't close your root session until you've verified the deploy user can SSH in!

---

## Step 3: Set Up GitHub Deploy Keys

On **each server**, create a dedicated SSH key for GitHub:

```bash
# SSH as the deploy user
ssh deploy@<droplet-ip>

# Generate a new SSH key for GitHub
ssh-keygen -t ed25519 -C "deploy@viralclipai-api" -f ~/.ssh/github_deploy -N ""

# Display the public key
cat ~/.ssh/github_deploy.pub
```

### Add the key to GitHub:

1. Go to **GitHub Repo** → **Settings** → **Deploy keys** → **Add deploy key**
2. Title: `viralclipai-api-deploy` (or `viralclipai-worker-deploy`)
3. Key: Paste the public key from the command above
4. ✅ Check "Allow write access" (optional, for git push)
5. Click **Add key**

> Repeat for both API and Worker servers with different key names.

### Configure SSH to use the key:

On each server, create/edit `~/.ssh/config`:

```bash
cat >> ~/.ssh/config << 'EOF'
Host github.com
    HostName github.com
    User git
    IdentityFile ~/.ssh/github_deploy
    IdentitiesOnly yes
EOF

chmod 600 ~/.ssh/config
```

### Test the connection:

```bash
ssh -T git@github.com
# Should output: "Hi <repo>! You've successfully authenticated..."
```

---

## Step 4: Clone the Repository

On **each server** (as deploy user):

```bash
cd /var/www
git clone git@github.com:vmatresu/viralclipai.git viralclipai-backend
cd viralclipai-backend
```

---

## Step 5: Create Managed Valkey Database

### In DigitalOcean Console:

1. Go to **Databases** → **Create Database**
2. Select **Valkey**
3. Configuration:
   - Region: NYC1 (same as droplets)
   - Plan: Basic ($15/mo, 1GB)
   - Name: `viralclipai-valkey`
4. Click **Create Database Cluster**

### Configure Trusted Sources:

1. Go to your Valkey database → **Settings** → **Trusted Sources**
2. Add both droplets by selecting them from the dropdown
3. This restricts access to only your servers

### Get Connection String:

From **Connection Details**, copy the connection string:
```
rediss://default:PASSWORD@viralclipai-valkey-xxxxx.db.ondigitalocean.com:25061
```

> Note: `rediss://` (double 's') means TLS is enabled.

---

## Step 6: Create Environment Files

### On the API Server:

```bash
cd /var/www/viralclipai-backend
cat > .env << 'EOF'
# Environment
ENVIRONMENT=production
RUST_LOG=info,tower_http=info

# CORS (allowed frontend origins)
CORS_ORIGINS=https://www.viralclipai.io,https://viralclipai.io

# Valkey/Redis (from DigitalOcean)
VALKEY_URL=rediss://default:PASSWORD@your-host.db.ondigitalocean.com:25061
REDIS_URL=rediss://default:PASSWORD@your-host.db.ondigitalocean.com:25061

# Firebase (from Firebase Console → Project Settings → Service Accounts)
FIREBASE_PROJECT_ID=your-project-id
FIREBASE_CLIENT_EMAIL=firebase-adminsdk-xxxxx@your-project.iam.gserviceaccount.com
FIREBASE_PRIVATE_KEY="-----BEGIN PRIVATE KEY-----\nYOUR_KEY_HERE\n-----END PRIVATE KEY-----\n"

# Cloudflare R2
R2_ENDPOINT_URL=https://xxx.r2.cloudflarestorage.com
R2_ACCESS_KEY_ID=your-access-key
R2_SECRET_ACCESS_KEY=your-secret-key
R2_BUCKET_NAME=viralclipai
R2_PUBLIC_URL=https://cdn.viralclipai.io

# JWT Secret (generate with: openssl rand -base64 32)
JWT_SECRET=your-jwt-secret-here
EOF

chmod 600 .env
```

### On the Worker Server:

```bash
cd /var/www/viralclipai-backend
cat > .env << 'EOF'
# Environment
ENVIRONMENT=production
RUST_LOG=info,tower_http=info

# Worker Config
WORKER_CONCURRENCY=4

# Valkey/Redis (same as API)
VALKEY_URL=rediss://default:PASSWORD@your-host.db.ondigitalocean.com:25061
REDIS_URL=rediss://default:PASSWORD@your-host.db.ondigitalocean.com:25061

# Firebase (same as API)
FIREBASE_PROJECT_ID=your-project-id
FIREBASE_CLIENT_EMAIL=firebase-adminsdk-xxxxx@your-project.iam.gserviceaccount.com
FIREBASE_PRIVATE_KEY="-----BEGIN PRIVATE KEY-----\nYOUR_KEY_HERE\n-----END PRIVATE KEY-----\n"

# Cloudflare R2 (same as API)
R2_ENDPOINT_URL=https://xxx.r2.cloudflarestorage.com
R2_ACCESS_KEY_ID=your-access-key
R2_SECRET_ACCESS_KEY=your-secret-key
R2_BUCKET_NAME=viralclipai
R2_PUBLIC_URL=https://cdn.viralclipai.io

# Gemini AI (optional, for captions)
GEMINI_API_KEY=your-gemini-key
EOF

chmod 600 .env
```

---

## Step 7: Configure GitHub Actions

### Add Repository Secrets:

Go to **GitHub** → **Your Repo** → **Settings** → **Secrets and variables** → **Actions**

Add these secrets:

| Secret Name | Value |
|-------------|-------|
| `API_HOST` | Your API droplet IP (e.g., `68.183.124.231`) |
| `WORKER_HOST` | Your Worker droplet IP |
| `DO_USER` | `deploy` |
| `DO_PORT` | `22` |
| `DO_SSH_KEY` | Your private SSH key (see below) |

### Get your SSH private key:

On your **local machine** (the key you use to SSH to servers):

```bash
cat ~/.ssh/id_ed25519
# or
cat ~/.ssh/id_rsa
```

Copy the entire content including the `-----BEGIN...` and `-----END...` lines.

---

## Step 8: Configure Nginx (API Server Only)

On the API server, configure Nginx as a reverse proxy:

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
```

### Set up SSL with Certbot:

```bash
sudo certbot --nginx -d api.viralclipai.io
```

---

## Step 9: Initial Deployment

### On the API Server:

```bash
cd /var/www/viralclipai-backend
docker compose -f deploy/docker-compose.api.yml up -d --build
```

### On the Worker Server:

```bash
cd /var/www/viralclipai-backend
docker compose -f deploy/docker-compose.worker.yml up -d --build
```

---

## Step 10: Verify Deployment

### Check API:

```bash
# On API server
curl http://localhost:8000/health

# From anywhere (after DNS/SSL setup)
curl https://api.viralclipai.io/health
```

### Check Worker:

```bash
# On Worker server
docker compose -f deploy/docker-compose.worker.yml logs -f
```

---

## Automatic Deployments

Once GitHub Actions is configured, deployments happen automatically:

- **API deploys** when changes are pushed to `backend/**`, `Dockerfile`, or `deploy/docker-compose.api.yml`
- **Worker deploys** when changes are pushed to `backend/**`, `Dockerfile`, or `deploy/docker-compose.worker.yml`

### Manual deployment:

Go to **GitHub** → **Actions** → Select workflow → **Run workflow**

---

## Useful Commands

### View logs:

```bash
# API
docker compose -f deploy/docker-compose.api.yml logs -f

# Worker
docker compose -f deploy/docker-compose.worker.yml logs -f
```

### Restart services:

```bash
# API
docker compose -f deploy/docker-compose.api.yml restart

# Worker
docker compose -f deploy/docker-compose.worker.yml restart
```

### Stop services:

```bash
docker compose -f deploy/docker-compose.api.yml down
docker compose -f deploy/docker-compose.worker.yml down
```

### Update manually:

```bash
cd /var/www/viralclipai-backend
git pull origin main
docker compose -f deploy/docker-compose.api.yml up -d --build
```

---

## Troubleshooting

### SSH Permission Denied:

```bash
# Check SSH keys are set up correctly
ls -la ~/.ssh/
cat ~/.ssh/authorized_keys

# Check SSH config
sudo cat /etc/ssh/sshd_config.d/hardening.conf
```

### Docker Permission Denied:

```bash
# Ensure deploy user is in docker group
groups deploy
# Should show: deploy : deploy sudo docker

# If not, add them:
sudo usermod -aG docker deploy
# Then logout and login again
```

### Container not starting:

```bash
# Check logs
docker compose -f deploy/docker-compose.api.yml logs --tail=100

# Check if env file exists
cat .env | head -5
```

### Valkey connection issues:

```bash
# Test connection from server
docker run --rm redis:alpine redis-cli -u "rediss://default:PASSWORD@host:25061" ping
```

---

## File Structure

```
deploy/
├── README.md                    # This file
├── server-hardening.sh          # Hardening script for API servers
├── server-hardening-worker.sh   # Hardening script for Worker servers
├── docker-compose.api.yml       # Docker Compose for API service
└── docker-compose.worker.yml    # Docker Compose for Worker service
```

---

## Security Checklist

- [ ] SSH key-only authentication enabled
- [ ] Root login disabled
- [ ] UFW firewall active
- [ ] fail2ban running
- [ ] Automatic security updates enabled
- [ ] Valkey trusted sources configured
- [ ] `.env` files have 600 permissions
- [ ] SSL certificates installed (API only)
