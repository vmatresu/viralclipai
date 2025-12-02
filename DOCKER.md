# Docker Setup Documentation

This document describes the Docker setup for the Viral Clip AI application, including best practices, build optimizations, and deployment strategies.

## Overview

The project uses multi-stage Docker builds for both backend (FastAPI) and frontend (Next.js) services, optimized for production with security hardening, layer caching, and minimal image sizes.

## Architecture

### Multi-Stage Builds

#### Backend (FastAPI)
1. **base** - Common Python configuration
2. **system-deps** - System packages (ffmpeg, build tools)
3. **python-deps** - Python dependencies (cached layer)
4. **app-code** - Application code
5. **prod-base** - Security hardening (non-root user)
6. **dev** - Development with hot reload
7. **prod** - Production with Gunicorn
8. **test** - Testing environment

#### Frontend (Next.js)
1. **base** - Common Node.js configuration
2. **deps** - npm dependencies (cached layer)
3. **builder** - Next.js build process
4. **runner** - Production runtime (standalone output)
5. **dev** - Development with hot reload
6. **lint** - Linting environment

## Build Optimizations

### Layer Caching
- Dependencies are installed in separate stages for better cache hits
- Requirements/package files are copied before source code
- System packages are installed in a single RUN command

### BuildKit Features
- Parallel builds with `DOCKER_BUILDKIT=1`
- Cache mounts for faster dependency installation
- Build secrets for sensitive data

### Image Size Optimization
- Multi-stage builds eliminate build dependencies from final image
- Alpine-based images for frontend (smaller footprint)
- Standalone Next.js output (minimal runtime files)
- Cleanup of package managers and caches

## Security Best Practices

### Non-Root Users
- Backend runs as `appuser` (UID 1000)
- Frontend runs as `nextjs` (UID 1000)
- Proper file permissions set

### Security Hardening
- Read-only root filesystem (where possible)
- Minimal base images
- No unnecessary packages
- Security headers in Next.js config
- Health checks for orchestration

### Secrets Management
- Environment variables via `.env` files
- Build-time secrets (not committed)
- Runtime secrets via Docker secrets (production)

## Usage

### Development

```bash
# Start development environment
make up-dev
# or
docker-compose -f docker-compose.dev.yml up -d

# View logs
make logs-dev
# or
docker-compose -f docker-compose.dev.yml logs -f

# Access shell
make shell-api-dev
make shell-web-dev
```

### Production

```bash
# Build production images
make build
# or
./.docker-build.sh all prod

# Start production environment
make up
# or
docker-compose up -d

# View logs
make logs
```

### Individual Service Builds

```bash
# Build API only
make build-api
./.docker-build.sh api prod

# Build Web only
make build-web
./.docker-build.sh web prod
```

## Docker Compose Files

### `docker-compose.yml` (Production)
- Production-optimized settings
- Resource limits
- Health checks
- Read-only filesystems
- Proper logging configuration

### `docker-compose.dev.yml` (Development)
- Volume mounts for hot reload
- Development-friendly settings
- More permissive resource limits
- Debug logging enabled

## Environment Variables

### Backend (.env.api)
```bash
ENVIRONMENT=production
LOG_LEVEL=INFO
R2_ACCOUNT_ID=...
R2_BUCKET_NAME=...
# ... other configs
```

### Frontend (web/.env.production)
```bash
NODE_ENV=production
NEXT_PUBLIC_API_BASE_URL=http://api:8000
NEXT_PUBLIC_FIREBASE_API_KEY=...
# ... other configs
```

## Volumes

### Production
- `api-logs` - Application logs
- `api-videos` - Video processing workspace

### Development
- Source code mounted for hot reload
- Separate dev volumes for logs/videos

## Health Checks

Both services include health check endpoints:
- API: `http://localhost:8000/health`
- Web: `http://localhost:3000/api/health`

Health checks are configured in docker-compose files with:
- 30s interval
- 10s timeout
- 3 retries
- 40s start period

## Resource Limits

### Production
- API: 2 CPU, 2GB RAM (limit) / 0.5 CPU, 512MB RAM (reservation)
- Web: 1 CPU, 512MB RAM (limit) / 0.25 CPU, 256MB RAM (reservation)

### Development
- API: 4 CPU, 4GB RAM
- Web: 2 CPU, 2GB RAM

## Build Arguments

### Common
- `BUILD_DATE` - ISO 8601 build timestamp
- `VCS_REF` - Git commit SHA
- `VERSION` - Version tag or "dev"

### Frontend Specific
- `NEXT_PUBLIC_API_BASE_URL` - API endpoint URL
- `NEXT_PUBLIC_FIREBASE_*` - Firebase configuration

## Troubleshooting

### Build Failures
1. Check Docker BuildKit is enabled: `export DOCKER_BUILDKIT=1`
2. Verify `.dockerignore` files are correct
3. Check for missing dependencies in requirements.txt/package.json
4. Review build logs: `docker-compose build --progress=plain`

### Runtime Issues
1. Check container logs: `make logs` or `docker-compose logs`
2. Verify health checks: `make health`
3. Check resource usage: `docker stats`
4. Verify environment variables: `docker-compose config`

### Performance Issues
1. Ensure BuildKit caching is working
2. Check layer cache hits in build output
3. Verify resource limits are appropriate
4. Monitor container resource usage

## CI/CD Integration

### GitHub Actions Example
```yaml
- name: Build Docker images
  run: |
    export DOCKER_BUILDKIT=1
    make ci-build
    make ci-build-web
```

### Docker Hub / Registry Push
```bash
docker tag viralclipai-api:latest your-registry/viralclipai-api:latest
docker push your-registry/viralclipai-api:latest
```

## Best Practices Checklist

- ✅ Multi-stage builds for minimal images
- ✅ Layer caching optimization
- ✅ Non-root users
- ✅ Health checks configured
- ✅ Resource limits set
- ✅ Security headers enabled
- ✅ Proper .dockerignore files
- ✅ BuildKit optimizations
- ✅ Read-only filesystems (where possible)
- ✅ Logging configuration
- ✅ Environment variable management
- ✅ Volume management
- ✅ Network isolation

## Additional Resources

- [Docker Best Practices](https://docs.docker.com/develop/dev-best-practices/)
- [Multi-stage Builds](https://docs.docker.com/build/building/multi-stage/)
- [BuildKit](https://docs.docker.com/build/buildkit/)
- [Security Best Practices](https://docs.docker.com/engine/security/)

