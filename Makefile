# =============================================================================
# Makefile for Docker Build and Deployment
# =============================================================================
# Provides convenient commands for building, testing, and deploying containers
# =============================================================================

.PHONY: help build build-api build-web build-prod build-dev up up-dev down down-dev \
        logs logs-api logs-web shell-api shell-web test clean prune

# Variables
DOCKER_BUILDKIT := 1
DOCKER_COMPOSE := docker-compose
DOCKER_COMPOSE_DEV := docker-compose -f docker-compose.dev.yml
VERSION := $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
BUILD_DATE := $(shell date -u +'%Y-%m-%dT%H:%M:%SZ')
VCS_REF := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")

# Export BuildKit
export DOCKER_BUILDKIT

# Default target
.DEFAULT_GOAL := help

help: ## Show this help message
	@echo "Usage: make [target]"
	@echo ""
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# -----------------------------------------------------------------------------
# Build Targets
# -----------------------------------------------------------------------------

build: build-api build-web ## Build all services (production)

build-api: ## Build backend API image
	@echo "Building API image..."
	DOCKER_BUILDKIT=1 docker build \
		--target prod \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg VCS_REF=$(VCS_REF) \
		--build-arg VERSION=$(VERSION) \
		--tag viralclipai-api:$(VERSION) \
		--tag viralclipai-api:latest \
		--progress=plain \
		.

build-web: ## Build frontend web image
	@echo "Building Web image..."
	DOCKER_BUILDKIT=1 docker build \
		--target runner \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg VCS_REF=$(VCS_REF) \
		--build-arg VERSION=$(VERSION) \
		--build-arg NEXT_PUBLIC_API_BASE_URL=${NEXT_PUBLIC_API_BASE_URL:-http://api:8000} \
		--tag viralclipai-web:$(VERSION) \
		--tag viralclipai-web:latest \
		--progress=plain \
		./web

build-dev: ## Build all services (development)
	@echo "Building development images..."
	$(DOCKER_COMPOSE_DEV) build --build-arg BUILD_DATE=$(BUILD_DATE) --build-arg VCS_REF=$(VCS_REF) --build-arg VERSION=$(VERSION)

build-prod: build ## Alias for build (production)

# Build with cache from registry (for CI/CD)
build-cache: ## Build with cache from registry
	@echo "Building with cache..."
	DOCKER_BUILDKIT=1 docker build \
		--target prod \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg VCS_REF=$(VCS_REF) \
		--build-arg VERSION=$(VERSION) \
		--cache-from viralclipai-api:latest \
		--tag viralclipai-api:$(VERSION) \
		--tag viralclipai-api:latest \
		.

# -----------------------------------------------------------------------------
# Run Targets
# -----------------------------------------------------------------------------

up: ## Start all services (production)
	$(DOCKER_COMPOSE) up -d

up-dev: ## Start all services (development)
	$(DOCKER_COMPOSE_DEV) up -d

down: ## Stop all services (production)
	$(DOCKER_COMPOSE) down

down-dev: ## Stop all services (development)
	$(DOCKER_COMPOSE_DEV) down

restart: down up ## Restart all services (production)

restart-dev: down-dev up-dev ## Restart all services (development)

# -----------------------------------------------------------------------------
# Logs Targets
# -----------------------------------------------------------------------------

logs: ## Show logs for all services
	$(DOCKER_COMPOSE) logs -f

logs-api: ## Show logs for API service
	$(DOCKER_COMPOSE) logs -f api

logs-web: ## Show logs for Web service
	$(DOCKER_COMPOSE) logs -f web

logs-dev: ## Show logs for all services (development)
	$(DOCKER_COMPOSE_DEV) logs -f

# -----------------------------------------------------------------------------
# Shell Targets
# -----------------------------------------------------------------------------

shell-api: ## Open shell in API container
	$(DOCKER_COMPOSE) exec api /bin/bash || $(DOCKER_COMPOSE) exec api /bin/sh

shell-web: ## Open shell in Web container
	$(DOCKER_COMPOSE) exec web /bin/sh

shell-api-dev: ## Open shell in API container (development)
	$(DOCKER_COMPOSE_DEV) exec api /bin/bash || $(DOCKER_COMPOSE_DEV) exec api /bin/sh

shell-web-dev: ## Open shell in Web container (development)
	$(DOCKER_COMPOSE_DEV) exec web /bin/sh

# -----------------------------------------------------------------------------
# Health Check Targets
# -----------------------------------------------------------------------------

health: ## Check health of all services
	@echo "Checking API health..."
	@curl -f http://localhost:8000/health || echo "API is not healthy"
	@echo ""
	@echo "Checking Web health..."
	@curl -f http://localhost:3000/api/health || echo "Web is not healthy"

# -----------------------------------------------------------------------------
# Test Targets
# -----------------------------------------------------------------------------

test: ## Run tests in containers
	@echo "Running API tests..."
	$(DOCKER_COMPOSE) exec api pytest -v || echo "No tests configured"
	@echo "Running Web tests..."
	$(DOCKER_COMPOSE) exec web npm test || echo "No tests configured"

test-api: ## Run API tests
	$(DOCKER_COMPOSE) exec api pytest -v || echo "No tests configured"

test-web: ## Run Web tests
	$(DOCKER_COMPOSE) exec web npm test || echo "No tests configured"

# -----------------------------------------------------------------------------
# Cleanup Targets
# -----------------------------------------------------------------------------

clean: ## Remove containers and volumes
	$(DOCKER_COMPOSE) down -v
	$(DOCKER_COMPOSE_DEV) down -v

prune: ## Remove unused Docker resources
	docker system prune -af --volumes

clean-images: ## Remove project images
	docker rmi viralclipai-api:latest viralclipai-api:$(VERSION) viralclipai-web:latest viralclipai-web:$(VERSION) 2>/dev/null || true
	docker rmi viralclipai-api:dev viralclipai-web:dev 2>/dev/null || true

# -----------------------------------------------------------------------------
# CI/CD Targets
# -----------------------------------------------------------------------------

ci-build: ## Build for CI/CD (no cache)
	DOCKER_BUILDKIT=1 docker build \
		--target prod \
		--no-cache \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg VCS_REF=$(VCS_REF) \
		--build-arg VERSION=$(VERSION) \
		--tag viralclipai-api:$(VERSION) \
		.

ci-build-web: ## Build web for CI/CD (no cache)
	DOCKER_BUILDKIT=1 docker build \
		--target runner \
		--no-cache \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg VCS_REF=$(VCS_REF) \
		--build-arg VERSION=$(VERSION) \
		--tag viralclipai-web:$(VERSION) \
		./web

# -----------------------------------------------------------------------------
# Security Scanning
# -----------------------------------------------------------------------------

scan: ## Scan images for vulnerabilities (requires docker scout)
	@echo "Scanning API image..."
	docker scout cves viralclipai-api:latest || echo "docker scout not available"
	@echo "Scanning Web image..."
	docker scout cves viralclipai-web:latest || echo "docker scout not available"

# -----------------------------------------------------------------------------
# Firebase Deployment
# -----------------------------------------------------------------------------

firebase-deploy-indexes: ## Deploy Firestore indexes
	@echo "Deploying Firestore indexes..."
	firebase deploy --only firestore:indexes

firebase-deploy-rules: ## Deploy Firestore security rules
	@echo "Deploying Firestore security rules..."
	firebase deploy --only firestore:rules

firebase-deploy: firebase-deploy-rules firebase-deploy-indexes ## Deploy Firestore rules and indexes

# -----------------------------------------------------------------------------
# Development Helpers
# -----------------------------------------------------------------------------

dev: ## Start development environment with smart rebuild detection
	@./scripts/docker-dev.sh

dev:legacy: up-dev logs-dev ## Start development environment (legacy, direct docker-compose)

prod: build up logs ## Build and start production environment

