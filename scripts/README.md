# Docker Development Scripts

## docker-dev.sh

Smart Docker development build script that automatically detects dependency changes and rebuilds containers without cache when needed.

### Features

- **Automatic Detection**: Checks if `package.json`, `package-lock.json`, or `requirements.txt` have changed
- **Dependency Validation**: Verifies that critical dependencies are installed in containers
- **Smart Caching**: Uses Docker cache when dependencies haven't changed, rebuilds without cache when they have
- **Force Rebuild**: Option to force a full rebuild without cache

### Usage

```bash
# Normal usage - automatically detects if rebuild is needed
./scripts/docker-dev.sh

# Force rebuild without cache
./scripts/docker-dev.sh --force
# or
./scripts/docker-dev.sh -f

# Via npm
npm run dev

# Via Makefile
make dev
```

### How It Works

1. **Checksum Calculation**: Calculates SHA256 checksums of dependency files (`package.json`, `package-lock.json`, `requirements.txt`)
2. **Cache Comparison**: Compares current checksums with cached checksums stored in `.docker-deps-cache`
3. **Container Validation**: Verifies that critical dependencies exist in running containers
4. **Smart Rebuild**: 
   - If checksums match and dependencies are valid → uses Docker cache (fast)
   - If checksums differ or dependencies missing → rebuilds without cache (ensures correctness)
5. **Cache Update**: Updates cache file after successful builds

### Cache File

The script maintains a cache file at `.docker-deps-cache` (gitignored) that stores checksums of dependency files. This file is automatically updated after successful builds.

### Troubleshooting

If you encounter dependency issues:

1. **Force rebuild**: Run `./scripts/docker-dev.sh --force` to rebuild everything without cache
2. **Clear cache**: Delete `.docker-deps-cache` file to force rebuild on next run
3. **Manual rebuild**: Use `docker-compose -f docker-compose.dev.yml build --no-cache <service>`

### Example Output

```
ℹ Checking if rebuild is needed...
ℹ package.json/package-lock.json changed for web service
⚠ Rebuilding web without cache...
ℹ Building web service...
✓ Successfully built web
ℹ Building api service...
✓ Successfully built api
ℹ Starting services...
ℹ Showing logs (Ctrl+C to exit)...
```

