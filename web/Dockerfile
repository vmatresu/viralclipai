# =============================================================================
# Multi-Stage Production Dockerfile for Next.js Frontend
# =============================================================================
# Best Practices:
# - Multi-stage builds for minimal final image
# - Layer caching optimization (dependencies separate from code)
# - Security hardening (non-root, minimal packages)
# - Build-time arguments for flexibility
# - Standalone output for smaller images
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Base - Common configuration
# -----------------------------------------------------------------------------
FROM node:24-alpine AS base

# Build arguments
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION=dev

# Labels for metadata
LABEL org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.authors="Viral Clip AI" \
      org.opencontainers.image.url="https://www.viralvideoai.io" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.vendor="Viral Clip AI" \
      org.opencontainers.image.title="Viral Clip AI Frontend" \
      org.opencontainers.image.description="Next.js frontend for video clip generation" \
      maintainer="Viral Clip AI Team"

# Disable telemetry and set working directory
ENV NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: Dependencies - Install npm packages
# -----------------------------------------------------------------------------
FROM base AS deps

# Copy package files first for better caching
# This layer will only rebuild if package files change
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* ./

# Install dependencies based on lockfile
# Use npm ci for faster, reliable, reproducible builds
# Use --legacy-peer-deps to handle peer dependency conflicts
RUN if [ -f package-lock.json ]; then \
        npm ci --legacy-peer-deps --ignore-scripts --prefer-offline --no-audit || \
        npm install --legacy-peer-deps --ignore-scripts --prefer-offline --no-audit; \
    elif [ -f yarn.lock ]; then \
        yarn install --frozen-lockfile --ignore-scripts --prefer-offline --non-interactive; \
    elif [ -f pnpm-lock.yaml ]; then \
        corepack enable && pnpm install --frozen-lockfile --ignore-scripts --prefer-offline; \
    else \
        npm install --legacy-peer-deps --ignore-scripts --prefer-offline --no-audit; \
    fi && \
    # Clean npm cache
    npm cache clean --force || true

# -----------------------------------------------------------------------------
# Stage 3: Builder - Build Next.js application
# -----------------------------------------------------------------------------
FROM base AS builder

ENV NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production \
    # Enable standalone output for smaller production image
    NEXT_STANDALONE=true

# Copy dependencies from deps stage
COPY --from=deps /app/node_modules ./node_modules

# Copy application source code
COPY . .

# Build arguments for build-time configuration
ARG NEXT_PUBLIC_API_BASE_URL
ARG NEXT_PUBLIC_FIREBASE_API_KEY
ARG NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN
ARG NEXT_PUBLIC_FIREBASE_PROJECT_ID

# Set build-time environment variables
ENV NEXT_PUBLIC_API_BASE_URL=${NEXT_PUBLIC_API_BASE_URL} \
    NEXT_PUBLIC_FIREBASE_API_KEY=${NEXT_PUBLIC_FIREBASE_API_KEY} \
    NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=${NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN} \
    NEXT_PUBLIC_FIREBASE_PROJECT_ID=${NEXT_PUBLIC_FIREBASE_PROJECT_ID}

# Build Next.js application
# This creates .next/standalone and .next/static
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 4: Runner - Production runtime image
# -----------------------------------------------------------------------------
FROM node:24-alpine AS runner

ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    # Security: Run as non-root
    UID=1000 \
    GID=1000

WORKDIR /app

# Install curl for health checks (lightweight)
RUN apk add --no-cache curl

# Create non-root user
RUN addgroup -g ${GID} -S nodejs && \
    adduser -S nextjs -u ${UID} -G nodejs

# Copy necessary files from builder
# Using standalone output for minimal image size
# Standalone output structure: .next/standalone contains server.js and node_modules
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static
COPY --from=builder --chown=nextjs:nodejs /app/public ./public

# Health check using curl (more reliable than node http)
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:3000/api/health || exit 1

# Switch to non-root user
USER nextjs

# Expose Next.js port
EXPOSE 3000

# Set environment variable for Next.js
ENV PORT=3000 \
    HOSTNAME="0.0.0.0"

# Production command - standalone output has server.js in root
CMD ["node", "server.js"]

# -----------------------------------------------------------------------------
# Stage 5: Development - Development image with hot reload
# -----------------------------------------------------------------------------
FROM base AS dev

ENV NODE_ENV=development \
    NEXT_TELEMETRY_DISABLED=1

# Copy dependencies
COPY --from=deps /app/node_modules ./node_modules

# Copy application code
COPY . .

# Expose Next.js dev port
EXPOSE 3000

# Development command with hot reload
CMD ["npm", "run", "dev"]

# -----------------------------------------------------------------------------
# Stage 6: Linter - Linting and code quality checks
# -----------------------------------------------------------------------------
FROM base AS lint

# Copy dependencies
COPY --from=deps /app/node_modules ./node_modules

# Copy application code
COPY . .

# Run linting
# CMD ["npm", "run", "lint"]
