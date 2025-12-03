import json
import asyncio
import logging
from pathlib import Path
from typing import Optional

from fastapi import WebSocket

from app.config import PROMPT_PATH, VIDEOS_DIR
from app.core.utils import extract_youtube_id, sanitize_filename, generate_run_id
from app.core.gemini import GeminiClient
from app.core import clipper
from app.core import saas, storage

logger = logging.getLogger(__name__)

async def process_video_workflow(
    websocket: WebSocket,
    url: str,
    style: str,
    user_id: Optional[str] = None,
    custom_prompt: Optional[str] = None,
    crop_mode: str = "none",
    target_aspect: str = "9:16",
):
    try:
        # Base prompt resolution order:
        # 1) user-provided custom prompt
        # 2) global admin-configured prompt in Firestore
        # 3) local prompt.txt fallback (for initial setups/migrations)
        if custom_prompt and custom_prompt.strip():
            base_prompt = custom_prompt.strip()
        else:
            global_prompt = saas.get_global_prompt()
            if global_prompt:
                base_prompt = global_prompt
            else:
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
        logger.info("Analyzing video with AI...")
        await websocket.send_json({"type": "log", "message": "🤖 Analyzing video with AI..."})
        
        client = GeminiClient()
        data = await asyncio.to_thread(client.get_highlights, base_prompt, url, workdir)
        
        logger.info("AI analysis complete.")
        await websocket.send_json({"type": "log", "message": "✅ AI analysis complete."})
        await websocket.send_json({"type": "progress", "value": 30})

        data["video_url"] = url
        if custom_prompt and custom_prompt.strip():
            data["custom_prompt"] = custom_prompt.strip()
        
        # Robustly find the highlights list, even if Gemini uses a different key (e.g. "emotional_moments")
        highlights = []
        if "highlights" in data and isinstance(data["highlights"], list):
            highlights = data["highlights"]
        else:
            # Fallback: Look for ANY list that looks like highlight data
            for key, value in data.items():
                if isinstance(value, list) and len(value) > 0:
                    first_item = value[0]
                    if isinstance(first_item, dict) and "start" in first_item and "end" in first_item:
                        logger.info(f"Found alternate highlights key: {key}")
                        highlights = value
                        break
        
        if not highlights:
            logger.warning("No valid highlights found in AI response.")
            await websocket.send_json({"type": "error", "message": "AI failed to identify clips in the video."})
            return

        for idx, h in enumerate(highlights, start=1):
            h.setdefault("id", idx)
            h.setdefault("priority", idx)
            h.setdefault("title", f"Clip {idx}")

        # Determine how many clips will be produced for plan enforcement
        # If style is 'all', we include the standard styles plus 'intelligent' if requested or by default?
        # The user wants 'intelligent' included in ALL.
        if style == "all":
            styles_to_process = clipper.AVAILABLE_STYLES + ["intelligent", "original"]
        else:
            styles_to_process = [style]
            
        total_clips = len(highlights) * len(styles_to_process)

        if user_id is not None and total_clips > 0:
            plan_id, max_clips = saas.get_plan_limits_for_user(user_id)
            used = saas.get_monthly_usage(user_id)
            if used + total_clips > max_clips:
                msg = (
                    f"Plan limit reached for user {user_id}: "
                    f"plan={plan_id}, used={used}, requested={total_clips}, max={max_clips}"
                )
                logger.warning(msg)
                await websocket.send_json(
                    {
                        "type": "error",
                        "message": "Clip limit reached for your current plan.",
                        "details": msg,
                    }
                )
                return

        highlights_path = workdir / "highlights.json"
        with open(highlights_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)

        # Upload analysis metadata to S3 for this user/run
        if user_id is not None:
            await asyncio.to_thread(
                storage.upload_file,
                highlights_path,
                f"{user_id}/{run_id}/highlights.json",
                "application/json",
            )

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
                
                # Determine effective crop mode
                # If the style is explicitly 'intelligent', force intelligent crop mode
                effective_crop_mode = "intelligent" if s == "intelligent" else crop_mode
                
                await asyncio.to_thread(
                    clipper.run_ffmpeg_clip_with_crop,
                    start,
                    end,
                    out_path,
                    s,
                    video_file,
                    effective_crop_mode,
                    target_aspect,
                    pad_before,
                    pad_after,
                )

                # Upload rendered clip and thumbnail to S3
                if user_id is not None:
                    s3_key = f"{user_id}/{run_id}/clips/{filename}"
                    await asyncio.to_thread(
                        storage.upload_file,
                        out_path,
                        s3_key,
                        "video/mp4",
                    )

                    thumb_path = out_path.with_suffix(".jpg")
                    if thumb_path.exists():
                        thumb_key = f"{user_id}/{run_id}/clips/{thumb_path.name}"
                        await asyncio.to_thread(
                            storage.upload_file,
                            thumb_path,
                            thumb_key,
                            "image/jpeg",
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

        # Record usage for this job
        if user_id is not None and total_clips > 0:
            video_title = data.get("video_title") or f"Video {youtube_id}"
            saas.record_video_job(
                user_id,
                run_id,
                url,
                video_title,
                total_clips,
                custom_prompt=custom_prompt.strip() if custom_prompt else None,
            )

        await websocket.send_json({"type": "progress", "value": 100})
        await websocket.send_json({"type": "log", "message": "✨ All done!"})
        await websocket.send_json({"type": "done", "videoId": run_id})

    except Exception as e:
        import traceback
        trace = traceback.format_exc()
        logger.error(f"Error processing video: {e}\n{trace}")
        await websocket.send_json({"type": "error", "message": str(e), "details": trace})
