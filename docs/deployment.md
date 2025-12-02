# Deployment

This document describes how to run Viral Clip AI in development and production.

## Local Development (Docker, recommended)

See `DOCKER_SETUP.md` for a detailed quickstart. In summary:

1. Create `.env.api.dev` in the project root (backend env).
2. Create `web/.env.local` in the `web/` folder (frontend env).
3. Place `firebase-credentials.json` in the project root.
4. Run:

```bash
docker-compose -f docker-compose.dev.yml up --build
```

- Backend API: `http://localhost:8000`
- Frontend: `http://localhost:3000`
- API docs: `http://localhost:8000/docs`

## Production (Docker Compose)

Production-like deployment uses `docker-compose.yml` and the multi-stage
Dockerfiles for the backend and frontend.

### Requirements

- Docker and docker-compose / `docker compose`.
- A provisioned VM (e.g. DigitalOcean Droplet) with:
  - `git`
  - `docker`
  - `docker-compose` or Docker CLI with compose plugin.

### Initial Provisioning (DigitalOcean example)

Two helper scripts are provided under `scripts/`:

- `scripts/do-vidclips-create-droplet.sh` – idempotently creates a droplet via
  `doctl`, waits for SSH, and optionally triggers provisioning.
- `scripts/do-vidclips-provision-backend.sh` – runs on the droplet to:
  - update & upgrade packages
  - install Docker and dependencies
  - configure a basic firewall with UFW
  - prepare the app directory

These scripts are optional but encode good defaults for production setups.

### GitHub Actions Deploy

The repo includes `.github/workflows/deploy.yml` which:

- Triggers on pushes to `main`.
- SSHes into the target droplet using `appleboy/ssh-action`.
- Resets the repo to `origin/main` in a fixed `APP_DIR`.
- Runs `docker compose -f docker-compose.yml up -d --build` to build and start
  the backend and frontend.
- Optionally prunes old Docker images.

You must configure the following GitHub secrets:

- `DO_HOST` – droplet hostname or IP.
- `DO_USER` – SSH user.
- `DO_PORT` – SSH port.
- `DO_SSH_KEY` – private key for SSH authentication.

### Environment Files for Production

On the server, you typically maintain:

- `.env.api` – backend env for production.
- `web/.env.production` – frontend env (if building in-place) or configured in
  your deployment platform (e.g. Vercel).

## Operational Notes

- Use health checks and monitoring around the Docker services.
- Ensure logs (`logs/app.log`) are rotated and shipped to your log platform.
- Configure proper DNS and TLS for your domains and API endpoint.
- Restrict inbound ports on the VM to HTTP(S) and SSH.

For storage configuration, see `docs/storage-and-media.md`. For logging, see
`docs/logging-and-observability.md`.
