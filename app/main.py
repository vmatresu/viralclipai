import uvicorn
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from app.routers import web
from app.version import __version__

app = FastAPI(title="Viral Clip AI", version=__version__)

# Mount static files if needed (currently mostly CDN)
# app.mount("/static", StaticFiles(directory=str(BASE_DIR / "static")), name="static")

# Include routers
app.include_router(web.router)

if __name__ == "__main__":
    uvicorn.run("app.main:app", host="0.0.0.0", port=8000, reload=True)
