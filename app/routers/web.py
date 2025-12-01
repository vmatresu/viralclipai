import json
import os
from datetime import datetime
from fastapi import APIRouter, Request, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse, FileResponse
from fastapi.templating import Jinja2Templates

from app.core.workflow import process_video_workflow
from app.config import VIDEOS_DIR, TEMPLATES_DIR

router = APIRouter()
templates = Jinja2Templates(directory=str(TEMPLATES_DIR))

@router.get("/", response_class=HTMLResponse)
async def index(request: Request):
    return templates.TemplateResponse("index.html", {"request": request})

@router.get("/history", response_class=HTMLResponse)
async def history(request: Request):
    videos = []
    if VIDEOS_DIR.exists():
        paths = sorted(VIDEOS_DIR.iterdir(), key=os.path.getmtime, reverse=True)
        for p in paths:
            if p.is_dir():
                json_path = p / "highlights.json"
                title = p.name
                url = ""
                timestamp = datetime.fromtimestamp(p.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
                
                if json_path.exists():
                    try:
                        data = json.loads(json_path.read_text(encoding="utf-8"))
                        highlights = data.get("highlights", [])
                        if highlights:
                            title = f"{len(highlights)} Clips Generated"
                        url = data.get("video_url", "")
                    except:
                        pass
                
                videos.append({
                    "id": p.name,
                    "title": title,
                    "url": url,
                    "date": timestamp
                })
    
    return templates.TemplateResponse("history.html", {"request": request, "videos": videos})

@router.websocket("/ws/process")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    try:
        data = await websocket.receive_json()
        url = data.get("url")
        style = data.get("style", "split")
        if not url:
            await websocket.send_json({"type": "error", "message": "No URL provided"})
            return
        
        await process_video_workflow(websocket, url, style)
    except WebSocketDisconnect:
        pass
    except Exception as e:
        try:
            await websocket.send_json({"type": "error", "message": str(e)})
        except:
            pass

@router.get("/api/videos/{video_id}")
async def get_video_info(video_id: str):
    workdir = VIDEOS_DIR / video_id
    if not workdir.exists():
        return {"error": "Video not found"}
    
    clips_dir = workdir / "clips"
    clips = []
    if clips_dir.exists():
        for f in sorted(clips_dir.glob("*.mp4")):
            size_mb = f.stat().st_size / (1024 * 1024)
            clips.append({
                "name": f.name,
                "url": f"/download/{video_id}/{f.name}",
                "size": f"{size_mb:.1f} MB"
            })
    
    return {"id": video_id, "clips": clips}

@router.get("/download/{video_id}/{filename}", response_class=FileResponse)
async def download_clip(video_id: str, filename: str):
    clip_path = VIDEOS_DIR / video_id / "clips" / filename
    if not clip_path.exists():
        return HTMLResponse("File not found", status_code=404)
    return FileResponse(clip_path, filename=filename, media_type="video/mp4")
