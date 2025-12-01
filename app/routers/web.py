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
                        
                        # Prefer the explicit video title from JSON
                        if data.get("video_title"):
                            title = data.get("video_title")
                        else:
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
    
    # Load highlights metadata
    highlights_map = {}
    json_path = workdir / "highlights.json"
    if json_path.exists():
        try:
            data = json.loads(json_path.read_text(encoding="utf-8"))
            for h in data.get("highlights", []):
                h_id = h.get("id")
                if h_id is not None:
                    highlights_map[int(h_id)] = {
                        "title": h.get("title", ""),
                        "description": h.get("reason", "") or h.get("description", "")
                    }
        except Exception as e:
            print(f"Error loading highlights.json: {e}")

    clips_dir = workdir / "clips"
    clips = []
    if clips_dir.exists():
        for f in sorted(clips_dir.glob("*.mp4")):
            size_mb = f.stat().st_size / (1024 * 1024)
            thumb_file = f.with_suffix(".jpg")
            thumb_url = f"/files/{video_id}/{thumb_file.name}" if thumb_file.exists() else None
            
            # Extract ID from filename: clip_{prio}_{id}_{title}_{style}.mp4
            # Example: clip_01_01_some-title_split.mp4
            title_text = f.name
            description_text = ""
            
            try:
                parts = f.name.split('_')
                if len(parts) >= 3 and parts[0] == "clip":
                    clip_id = int(parts[2])
                    if clip_id in highlights_map:
                        title_text = highlights_map[clip_id]["title"]
                        description_text = highlights_map[clip_id]["description"]
            except Exception:
                pass # Fallback to filename if parsing fails

            clips.append({
                "name": f.name,
                "title": title_text,
                "description": description_text,
                "url": f"/files/{video_id}/{f.name}",
                "thumbnail": thumb_url,
                "size": f"{size_mb:.1f} MB"
            })
    
    return {"id": video_id, "clips": clips}

@router.get("/files/{video_id}/{filename}", response_class=FileResponse)
async def serve_file(video_id: str, filename: str):
    clip_path = VIDEOS_DIR / video_id / "clips" / filename
    if not clip_path.exists():
        return HTMLResponse("File not found", status_code=404)
    
    media_type = "application/octet-stream"
    if filename.endswith(".mp4"):
        media_type = "video/mp4"
    elif filename.endswith(".jpg"):
        media_type = "image/jpeg"
        
    return FileResponse(clip_path, filename=filename, media_type=media_type)
