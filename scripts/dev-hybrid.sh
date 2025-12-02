#!/bin/bash

# =============================================================================
# Hybrid Development Script
# =============================================================================
# Starts the Backend API in Docker and provides instructions for the Frontend.
# =============================================================================

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}ℹ Checking if API rebuild is needed...${NC}"

# Build and start only the API service using the new backend-specific compose file
docker-compose -f docker-compose.backend.yml up -d --build api

echo -e "${GREEN}✓ Backend API started in Docker on port 8000${NC}"
echo -e "${BLUE}ℹ Showing API logs (Press Ctrl+C to stop following logs, backend will keep running)${NC}"

# Hint for the user
echo -e "${YELLOW}👉 Open a new terminal tab and run 'cd web && npm run dev' to start the frontend!${NC}"

# Follow logs
docker-compose -f docker-compose.backend.yml logs -f api
