# Docker Local Setup Guide

## Quick Start

### You Only Need 2 Environment Files

**1. Backend** - Create `.env.api.dev` in the **project root** (same folder as `docker-compose.dev.yml`):

```bash
# Required: Gemini API Key
GEMINI_API_KEY=your-gemini-api-key-here

# Required: Firebase Admin (for backend)
FIREBASE_PROJECT_ID=your-firebase-project-id
FIREBASE_CREDENTIALS_PATH=/app/firebase-credentials.json

# Required: Cloudflare R2 (S3-compatible storage)
R2_ACCOUNT_ID=your-r2-account-id
R2_BUCKET_NAME=your-bucket-name
R2_ACCESS_KEY_ID=your-r2-access-key
R2_SECRET_ACCESS_KEY=your-r2-secret-key
R2_ENDPOINT_URL=https://your-account-id.r2.cloudflarestorage.com

# Optional: Security (for local dev)
ALLOWED_HOSTS=localhost,127.0.0.1
CORS_ORIGINS=http://localhost:3000,http://localhost:8000
```

**2. Frontend** - Create `web/.env.local` in the **web folder**:

```bash
# Backend API URL (Docker network - use 'api' hostname)
NEXT_PUBLIC_API_BASE_URL=http://api:8000

# Required: Firebase Web SDK config
NEXT_PUBLIC_FIREBASE_API_KEY=your-firebase-web-api-key
NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=your-project.firebaseapp.com
NEXT_PUBLIC_FIREBASE_PROJECT_ID=your-firebase-project-id
NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET=your-project.appspot.com
NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID=your-sender-id
NEXT_PUBLIC_FIREBASE_APP_ID=your-app-id

# Optional: Firebase Analytics (Google Analytics 4)
NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID=G-XXXXXXXXXX
```

**File Structure:**

```
vidclips-gemini/
├── .env.api.dev          ← Backend env file (create this)
├── docker-compose.dev.yml
├── web/
│   └── .env.local        ← Frontend env file (create this)
└── ...
```

### 2. Place Firebase Credentials

If using Firebase Admin, place your Firebase service account JSON file in the project root as `firebase-credentials.json`. This will be mounted into the container.

### 3. Start with Docker Compose

Run the development setup:

```bash
docker-compose -f docker-compose.dev.yml up --build
```

This will:

- Build both backend (FastAPI) and frontend (Next.js) containers
- Start the API on `http://localhost:8000`
- Start the web frontend on `http://localhost:3000`
- Mount your code for hot-reload during development

### 4. Access the Application

- **Frontend**: http://localhost:3000
- **Backend API**: http://localhost:8000
- **API Docs**: http://localhost:8000/docs (FastAPI Swagger UI)

### 5. Stop the Containers

```bash
docker-compose -f docker-compose.dev.yml down
```

## Troubleshooting

### Port Already in Use

If ports 3000 or 8000 are already in use, you can modify the ports in `docker-compose.dev.yml`:

```yaml
ports:
  - "3001:3000" # Change host port
```

### Environment Variables Not Loading

- Make sure `.env.api.dev` is in the **project root** (same directory as `docker-compose.dev.yml`)
- Make sure `web/.env.local` is in the **web/** directory
- Restart containers after changing env files: `docker-compose -f docker-compose.dev.yml restart`

## Summary: Just 2 Files Needed

For local development, you only need:

1. **`.env.api.dev`** - Backend environment variables (project root)
2. **`web/.env.local`** - Frontend environment variables (web folder)

That's it! The other `.env` files (like `.env.api` or `web/.env.production`) are only for production builds.

### Firebase Credentials Not Found

- Ensure `firebase-credentials.json` exists in the project root
- Check that `FIREBASE_CREDENTIALS_PATH=/app/firebase-credentials.json` in `.env.api.dev`

### Frontend Can't Connect to Backend

- In Docker, the frontend uses `http://api:8000` (internal Docker network)
- If running frontend locally (not in Docker), use `http://localhost:8000` instead

## Production Build

For production-like testing:

```bash
docker-compose up --build
```

This uses the production Dockerfiles with optimized builds.
