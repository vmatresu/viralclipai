#!/bin/bash
# =============================================================================
# Smart Docker Development Build Script
# =============================================================================
# Automatically detects dependency issues and rebuilds without cache if needed
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WEB_DIR="$PROJECT_ROOT/web"
DOCKER_COMPOSE_DEV="$PROJECT_ROOT/docker-compose.dev.yml"
CACHE_FILE="$PROJECT_ROOT/.docker-deps-cache"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored messages
info() { echo -e "${BLUE}ℹ${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
warn() { echo -e "${YELLOW}⚠${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1"; }

# Function to calculate checksum of package files
calculate_checksum() {
    local dir="$1"
    if [ -f "$dir/package.json" ] && [ -f "$dir/package-lock.json" ]; then
        cat "$dir/package.json" "$dir/package-lock.json" | sha256sum | cut -d' ' -f1
    elif [ -f "$dir/package.json" ]; then
        cat "$dir/package.json" | sha256sum | cut -d' ' -f1
    else
        echo ""
    fi
}

# Function to check if dependencies are installed in container
check_container_deps() {
    local service="$1"
    local container_name="${service}-dev"
    
    # Check if container exists and is running
    if ! docker ps --format '{{.Names}}' | grep -q "^${container_name}$"; then
        return 1
    fi
    
    # Check for critical dependencies
    case "$service" in
        web)
            # Check for critical npm packages
            if ! docker exec "$container_name" sh -c "test -d /app/node_modules/tailwindcss-animate && test -d /app/node_modules/lucide-react && test -d /app/node_modules/next-themes" 2>/dev/null; then
                return 1
            fi
            ;;
        api)
            # Check for critical Python packages (if needed)
            if ! docker exec "$container_name" sh -c "test -d /app/venv" 2>/dev/null; then
                return 1
            fi
            ;;
    esac
    
    return 0
}

# Function to check if rebuild is needed
needs_rebuild() {
    local service="$1"
    local current_checksum
    
    case "$service" in
        web)
            current_checksum=$(calculate_checksum "$WEB_DIR")
            if [ -z "$current_checksum" ]; then
                warn "No package.json found for web service"
                return 1
            fi
            
            # Read cached checksum
            if [ -f "$CACHE_FILE" ]; then
                local cached_checksum=$(grep "^web:" "$CACHE_FILE" | cut -d':' -f2 || echo "")
                if [ "$current_checksum" != "$cached_checksum" ]; then
                    info "package.json/package-lock.json changed for web service"
                    return 0
                fi
            else
                info "No cache file found, will rebuild"
                return 0
            fi
            
            # Check if container has dependencies
            if ! check_container_deps "$service"; then
                warn "Container dependencies missing or invalid for $service"
                return 0
            fi
            
            return 1
            ;;
        api)
            # For API, check requirements.txt
            if [ -f "$PROJECT_ROOT/requirements.txt" ]; then
                local current_checksum=$(cat "$PROJECT_ROOT/requirements.txt" | sha256sum | cut -d' ' -f1)
                if [ -f "$CACHE_FILE" ]; then
                    local cached_checksum=$(grep "^api:" "$CACHE_FILE" | cut -d':' -f2 || echo "")
                    if [ "$current_checksum" != "$cached_checksum" ]; then
                        info "requirements.txt changed for api service"
                        return 0
                    fi
                else
                    return 0
                fi
            fi
            
            if ! check_container_deps "$service"; then
                warn "Container dependencies missing or invalid for $service"
                return 0
            fi
            
            return 1
            ;;
    esac
}

# Function to update cache file
update_cache() {
    local service="$1"
    local checksum
    
    case "$service" in
        web)
            checksum=$(calculate_checksum "$WEB_DIR")
            ;;
        api)
            if [ -f "$PROJECT_ROOT/requirements.txt" ]; then
                checksum=$(cat "$PROJECT_ROOT/requirements.txt" | sha256sum | cut -d' ' -f1)
            else
                checksum=""
            fi
            ;;
    esac
    
    if [ -n "$checksum" ]; then
        # Remove old entry if exists
        if [ -f "$CACHE_FILE" ]; then
            grep -v "^${service}:" "$CACHE_FILE" > "${CACHE_FILE}.tmp" 2>/dev/null || true
            mv "${CACHE_FILE}.tmp" "$CACHE_FILE" 2>/dev/null || true
        fi
        # Add new entry
        echo "${service}:${checksum}" >> "$CACHE_FILE"
    fi
}

# Main build function
build_service() {
    local service="$1"
    local use_cache="${2:-true}"
    
    info "Building $service service..."
    
    if [ "$use_cache" = "false" ]; then
        warn "Rebuilding $service without cache..."
        docker-compose -f "$DOCKER_COMPOSE_DEV" build --no-cache "$service" || {
            error "Failed to build $service"
            return 1
        }
    else
        docker-compose -f "$DOCKER_COMPOSE_DEV" build "$service" || {
            error "Failed to build $service with cache, trying without cache..."
            docker-compose -f "$DOCKER_COMPOSE_DEV" build --no-cache "$service" || {
                error "Failed to build $service even without cache"
                return 1
            }
        }
    fi
    
    # Update cache after successful build
    update_cache "$service"
    success "Successfully built $service"
}

# Main execution
main() {
    cd "$PROJECT_ROOT"
    
    # Handle force rebuild flag
    local force_rebuild=false
    if [ "${1:-}" = "--force" ] || [ "${1:-}" = "-f" ]; then
        force_rebuild=true
        warn "Force rebuild requested - will rebuild all services without cache"
        rm -f "$CACHE_FILE"
    fi
    
    info "Checking if rebuild is needed..."
    
    local rebuild_web=false
    local rebuild_api=false
    
    if [ "$force_rebuild" = "true" ]; then
        rebuild_web=true
        rebuild_api=true
    else
        # Check web service
        if needs_rebuild "web"; then
            rebuild_web=true
        fi
        
        # Check api service
        if needs_rebuild "api"; then
            rebuild_api=true
        fi
    fi
    
    # Build services
    if [ "$rebuild_web" = "true" ]; then
        build_service "web" "false"
    else
        build_service "web" "true"
    fi
    
    if [ "$rebuild_api" = "true" ]; then
        build_service "api" "false"
    else
        build_service "api" "true"
    fi
    
    # Start services
    info "Starting services..."
    docker-compose -f "$DOCKER_COMPOSE_DEV" up -d || {
        error "Failed to start services"
        exit 1
    }
    
    # Show logs
    info "Showing logs (Ctrl+C to exit)..."
    docker-compose -f "$DOCKER_COMPOSE_DEV" logs -f
}

# Run main function
main "$@"

