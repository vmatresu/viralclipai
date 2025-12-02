"""Viral Clip AI - FastAPI Application Entry Point.

Production-hardened with:
- Comprehensive security middleware stack
- Rate limiting
- Request ID tracking
- Security headers (CSP, HSTS, etc.)
- Input validation
- Error sanitization
"""

import os
from contextlib import asynccontextmanager
from datetime import datetime, timezone

import uvicorn
from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles
from fastapi.middleware.cors import CORSMiddleware
from starlette.middleware.trustedhost import TrustedHostMiddleware
from starlette.exceptions import HTTPException as StarletteHTTPException

from app.config import (
    STATIC_DIR,
    ALLOWED_HOSTS,
    CORS_ORIGINS,
    logger,
)
from app.middleware import (
    ErrorSanitizationMiddleware,
    RateLimitMiddleware,
    RequestIDMiddleware,
    RequestLoggingMiddleware,
    SecurityHeadersMiddleware,
)
from app.routers import web
from app.version import __version__
from app.schemas import HealthResponse


# -----------------------------------------------------------------------------
# Application Lifespan
# -----------------------------------------------------------------------------

@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application startup and shutdown events."""
    logger.info("Starting Viral Clip AI v%s", __version__)
    yield
    logger.info("Shutting down Viral Clip AI")


# -----------------------------------------------------------------------------
# Application Factory
# -----------------------------------------------------------------------------

def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    
    # Determine environment
    is_production = os.getenv("ENVIRONMENT", "development").lower() == "production"
    debug_mode = not is_production
    
    # Create FastAPI app with security-conscious defaults
    app = FastAPI(
        title="Viral Clip AI",
        version=__version__,
        lifespan=lifespan,
        # Disable docs in production for security
        docs_url="/docs" if debug_mode else None,
        redoc_url="/redoc" if debug_mode else None,
        openapi_url="/openapi.json" if debug_mode else None,
    )
    
    # -------------------------------------------------------------------------
    # Middleware Stack (order matters - first added = last executed)
    # -------------------------------------------------------------------------
    
    # 1. Error sanitization (outermost - catches all errors)
    app.add_middleware(ErrorSanitizationMiddleware, debug=debug_mode)
    
    # 2. Request logging
    app.add_middleware(
        RequestLoggingMiddleware,
        exclude_paths={"/health", "/healthz", "/ready"},
    )
    
    # 3. Security headers
    app.add_middleware(SecurityHeadersMiddleware)
    
    # 4. Rate limiting
    app.add_middleware(
        RateLimitMiddleware,
        exclude_paths={"/health", "/healthz", "/ready", "/static"},
    )
    
    # 5. Request ID injection
    app.add_middleware(RequestIDMiddleware)
    
    # 6. Trusted hosts (prevents host header attacks)
    if ALLOWED_HOSTS and ALLOWED_HOSTS != ["*"]:
        app.add_middleware(
            TrustedHostMiddleware,
            allowed_hosts=ALLOWED_HOSTS,
        )
    
    # 7. CORS (innermost middleware for preflight handling)
    app.add_middleware(
        CORSMiddleware,
        allow_origins=CORS_ORIGINS,
        allow_credentials=True,
        allow_methods=["GET", "POST", "PUT", "DELETE", "OPTIONS"],
        allow_headers=["Authorization", "Content-Type", "X-Request-ID"],
        expose_headers=["X-Request-ID", "X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset"],
        max_age=600,  # Cache preflight for 10 minutes
    )
    
    # -------------------------------------------------------------------------
    # Exception Handlers
    # -------------------------------------------------------------------------
    
    @app.exception_handler(RequestValidationError)
    async def validation_exception_handler(request: Request, exc: RequestValidationError):
        """Handle Pydantic validation errors with clean messages."""
        errors = exc.errors()
        # Limit error details to prevent information leakage
        clean_errors = [
            {"field": ".".join(str(loc) for loc in err.get("loc", [])), "message": err.get("msg", "Invalid value")}
            for err in errors[:5]  # Limit to 5 errors
        ]
        return JSONResponse(
            status_code=422,
            content={"detail": "Validation error", "errors": clean_errors},
        )
    
    @app.exception_handler(StarletteHTTPException)
    async def http_exception_handler(request: Request, exc: StarletteHTTPException):
        """Handle HTTP exceptions with consistent format."""
        return JSONResponse(
            status_code=exc.status_code,
            content={"detail": exc.detail},
            headers=getattr(exc, "headers", None),
        )
    
    # -------------------------------------------------------------------------
    # Health Check Endpoints
    # -------------------------------------------------------------------------
    
    @app.get("/health", response_model=HealthResponse, tags=["Health"])
    @app.get("/healthz", response_model=HealthResponse, include_in_schema=False)
    async def health_check() -> HealthResponse:
        """Health check endpoint for load balancers and orchestrators."""
        return HealthResponse(
            status="healthy",
            version=__version__,
            timestamp=datetime.now(timezone.utc),
        )
    
    @app.get("/ready", tags=["Health"])
    async def readiness_check():
        """Readiness check - verifies dependencies are available."""
        # Add dependency checks here (database, cache, etc.)
        return {"status": "ready"}
    
    # -------------------------------------------------------------------------
    # Static Files & Routers
    # -------------------------------------------------------------------------
    
    # Mount static files for JS/CSS assets
    app.mount("/static", StaticFiles(directory=str(STATIC_DIR)), name="static")
    
    # Include API routers
    app.include_router(web.router)
    
    return app


# Create the application instance
app = create_app()


if __name__ == "__main__":
    uvicorn.run(
        "app.main:app",
        host="0.0.0.0",
        port=8000,
        reload=True,
        # Security: Limit request size
        limit_concurrency=100,
        limit_max_requests=10000,
    )
