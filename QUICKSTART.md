# Docker Quick Start Guide

## Prerequisites

- Docker Engine 20.10+ with BuildKit enabled
- Docker Compose 2.0+
- Make (optional, for convenience commands)

## Quick Commands

### Development

```bash
# Start development environment
make up-dev
# or
docker-compose -f docker-compose.dev.yml up -d

# View logs
make logs-dev

# Stop
make down-dev
```

### Production

```bash
# Build and start
make build && make up
# or
./.docker-build.sh all prod && docker-compose up -d

# View logs
make logs

# Stop
make down
```

## Environment Setup

1. Copy environment files:
   ```bash
   cp .env.api.example .env.api
   cp web/.env.production.example web/.env.production
   ```

2. Fill in required values in `.env.api` and `web/.env.production`

3. Start services:
   ```bash
   make up-dev  # Development
   make up      # Production
   ```

## Access Services

- **API**: http://localhost:8000
- **Web**: http://localhost:3000
- **API Docs**: http://localhost:8000/docs (development only)
- **Health**: http://localhost:8000/health

## Common Tasks

```bash
# Build specific service
make build-api
make build-web

# View logs
make logs-api
make logs-web

# Access shell
make shell-api
make shell-web

# Check health
make health

# Clean everything
make clean
```

## Troubleshooting

### Build fails
- Ensure BuildKit is enabled: `export DOCKER_BUILDKIT=1`
- Check `.dockerignore` files
- Verify all dependencies are in `requirements.txt` / `package.json`

### Services won't start
- Check logs: `make logs`
- Verify environment variables are set
- Check port availability: `lsof -i :8000` or `lsof -i :3000`

### Hot reload not working (dev)
- Ensure volumes are mounted correctly
- Check file permissions
- Verify `WATCHPACK_POLLING=true` is set (web service)

For more details, see [DOCKER.md](./DOCKER.md)

