# Frontend (Next.js) multi-stage image

FROM node:20-alpine AS deps
WORKDIR /app

# Install dependencies based on lockfile if present
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* ./
RUN npm install --production=false --ignore-scripts || npm install --ignore-scripts

FROM node:20-alpine AS builder
WORKDIR /app
ENV NEXT_TELEMETRY_DISABLED=1

COPY --from=deps /app/node_modules ./node_modules
COPY . .

RUN npm run build

FROM node:20-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1

# Only copy what we need to run the app
COPY --from=builder /app/public ./public
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./package.json

EXPOSE 3000
CMD ["npm", "run", "start"]

# Development target (used by docker-compose.dev.yml)
FROM node:20-alpine AS dev
WORKDIR /app
ENV NEXT_TELEMETRY_DISABLED=1

COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* ./
RUN npm install --ignore-scripts

COPY . .

EXPOSE 3000
CMD ["npm", "run", "dev"]
