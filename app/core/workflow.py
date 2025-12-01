import json
import asyncio
import logging
from pathlib import Path

from fastapi import WebSocket

from app.config import PROMPT_PATH, VIDEOS_DIR
from app.core.utils import extract_youtube_id, sanitize_filename, generate_run_id
from app.core.gemini import GeminiClient
from app.core import clipper

logger = logging.getLogger(__name__)

async def process_video_workflow(websocket: WebSocket, url: str, style: str):
    try:
        if not PROMPT_PATH.exists():
            raise RuntimeError(f"prompt.txt not found at {PROMPT_PATH}")

        base_prompt = PROMPT_PATH.read_text(encoding="utf-8")
        # We still extract ID for info, but not for folder name
        youtube_id = extract_youtube_id(url)
        
        # Create unique run folder
        run_id = generate_run_id()
        workdir = VIDEOS_DIR / run_id
        workdir.mkdir(parents=True, exist_ok=True)
        
        logger.info(f"Starting job for Video ID: {youtube_id} in {workdir}")
        await websocket.send_json({"type": "log", "message": f"🚀 Starting job for Video ID: {youtube_id}"})
        await websocket.send_json({"type": "progress", "value": 10})

        # 1. Gemini Analysis
        logger.info("Asking Gemini to analyze video...")
        await websocket.send_json({"type": "log", "message": "🤖 Asking Gemini to analyze video..."})
        
        client = GeminiClient()
        data = await asyncio.to_thread(client.get_highlights, base_prompt, url, workdir)
        
        logger.info("Gemini analysis complete.")
        await websocket.send_json({"type": "log", "message": "✅ Gemini analysis complete."})
        await websocket.send_json({"type": "progress", "value": 30})

        data["video_url"] = url
        highlights = data.get("highlights", [])
        for idx, h in enumerate(highlights, start=1):
            h.setdefault("id", idx)
            h.setdefault("priority", idx)
            h.setdefault("title", f"Clip {idx}")
        
        highlights_path = workdir / "highlights.json"
        with open(highlights_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)

        # 2. Download Video
        video_file = workdir / "source.mp4"
        clips_dir = clipper.ensure_dirs(workdir)
        
        logger.info("Downloading video...")
        await websocket.send_json({"type": "log", "message": "📥 Downloading video..."})
        
        await asyncio.to_thread(clipper.download_video, url, video_file)
        
        logger.info("Download complete.")
        await websocket.send_json({"type": "log", "message": "✅ Download complete."})
        await websocket.send_json({"type": "progress", "value": 50})

        # 3. Clipping
        styles_to_process = clipper.AVAILABLE_STYLES if style == "all" else [style]
        total_clips = len(highlights) * len(styles_to_process)
        completed_clips = 0

        for h in highlights:
            clip_id = h.get("id")
            title = h.get("title", f"clip_{clip_id}")
            start = h["start"]
            end = h["end"]
            pad_before = float(h.get("pad_before_seconds", 0) or 0)
            pad_after = float(h.get("pad_after_seconds", 0) or 0)
            prio = h.get("priority", 99)
            safe_title = sanitize_filename(title)

            for s in styles_to_process:
                filename = f"clip_{prio:02d}_{clip_id:02d}_{safe_title}_{s}.mp4"
                out_path = clips_dir / filename
                
                logger.info(f"Rendering clip: {title} ({s})")
                await websocket.send_json({"type": "log", "message": f"✂️ Rendering clip: {title} ({s})"})
                
                await asyncio.to_thread(
                    clipper.run_ffmpeg_clip,
                    start,
                    end,
                    out_path,
                    s,
                    video_file,
                    pad_before,
                    pad_after,
                )
                
                completed_clips += 1
                progress = 50 + int((completed_clips / total_clips) * 40)
                await websocket.send_json({"type": "progress", "value": progress})

        # 4. Cleanup
        if video_file.exists():
            video_file.unlink()
            logger.info("Cleaned up source video file.")
            await websocket.send_json({"type": "log", "message": "🧹 Cleaned up source video file."})

        logger.info("Job complete.")
        await websocket.send_json({"type": "progress", "value": 100})
        await websocket.send_json({"type": "log", "message": "✨ All done!"})
        await websocket.send_json({"type": "done", "videoId": run_id})

    except Exception as e:
        import traceback
        trace = traceback.format_exc()
        logger.error(f"Error processing video: {e}\n{trace}")
        await websocket.send_json({"type": "error", "message": str(e), "details": trace})
