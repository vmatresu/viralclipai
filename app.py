import os
import json
import subprocess
from pathlib import Path
from typing import List

from fastapi import FastAPI, Request, Form
from fastapi.responses import HTMLResponse, FileResponse, RedirectResponse

from gemini_client import GeminiClient

BASE_DIR = Path(__file__).parent.resolve()
VIDEOS_DIR = BASE_DIR / "videos"
PROMPT_PATH = BASE_DIR / "prompt.txt"
CLIPPER_PATH = BASE_DIR / "clipper.py"


def extract_youtube_id(url: str) -> str:
    """
    Very simple YouTube ID extraction.
    For production you may want a more robust parser.
    """
    import urllib.parse as up

    parsed = up.urlparse(url)
    if parsed.netloc in {"youtu.be"}:
        return parsed.path.lstrip("/")

    if "youtube.com" in parsed.netloc:
        qs = up.parse_qs(parsed.query)
        if "v" in qs:
            return qs["v"][0]

        # /shorts/<id> or /embed/<id>
        parts = [p for p in parsed.path.split("/") if p]
        if parts and parts[0] in {"shorts", "embed"} and len(parts) > 1:
            return parts[1]

    # Fallback to a safe file-name-ish hash
    return "video_" + str(abs(hash(url)))


def create_video_workdir(youtube_id: str) -> Path:
    workdir = VIDEOS_DIR / youtube_id
    workdir.mkdir(parents=True, exist_ok=True)
    return workdir


def run_clipper(workdir: Path, style: str = "split"):
    cmd = [
        "python",
        str(CLIPPER_PATH),
        "--workdir",
        str(workdir),
        "--style",
        style,
    ]
    subprocess.run(cmd, check=True)


def generate_highlights(url: str, style: str = "split") -> Path:
    if not PROMPT_PATH.exists():
        raise RuntimeError(f"prompt.txt not found at {PROMPT_PATH}")

    base_prompt = PROMPT_PATH.read_text(encoding="utf-8")
    youtube_id = extract_youtube_id(url)
    workdir = create_video_workdir(youtube_id)

    client = GeminiClient()
    data = client.get_highlights(base_prompt, url)

    # Ensure minimal fields
    data["video_url"] = url
    for idx, h in enumerate(data.get("highlights", []), start=1):
        h.setdefault("id", idx)
        h.setdefault("priority", idx)
        h.setdefault("title", f"Clip {idx}")
        h.setdefault("type", "interaction")

    highlights_path = workdir / "highlights.json"
    with open(highlights_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

    # Run clipper to download + cut + cleanup
    run_clipper(workdir, style=style)
    return workdir


# ---------------- CLI entry ---------------- #

def cli_main():
    import argparse

    parser = argparse.ArgumentParser(description="YouTube to Gemini highlights + clips app.")
    parser.add_argument("url", help="YouTube URL")
    parser.add_argument(
        "--style",
        default="split",
        choices=["split", "left_focus", "right_focus", "all"],
        help="Video style for clips.",
    )
    args = parser.parse_args()

    workdir = generate_highlights(args.url, style=args.style)
    print(f"[done] Result in: {workdir}")
    print(f" - highlights.json")
    print(f" - clips/ (rendered clips)")


# ---------------- Web UI ---------------- #

app = FastAPI(title="YT Gemini Clipper")


@app.get("/", response_class=HTMLResponse)
async def index(request: Request):
    # List all processed videos
    VIDEOS_DIR.mkdir(exist_ok=True)
    video_ids: List[str] = sorted([p.name for p in VIDEOS_DIR.iterdir() if p.is_dir()])

    html = """
    <html>
    <head>
        <title>YT Gemini Clipper</title>
        <style>
            body { font-family: system-ui, -apple-system, BlinkMacSystemFont, sans-serif; margin: 40px; }
            input[type=text] { width: 400px; padding: 8px; }
            button { padding: 8px 16px; }
            .video-card { border: 1px solid #ddd; padding: 12px; margin-top: 12px; border-radius: 6px; }
            .clip-list { margin-top: 8px; }
            .clip-item { margin-bottom: 4px; }
            .title { font-weight: 600; }
            .description { font-size: 0.9rem; color: #555; margin-top: 2px; }
        </style>
    </head>
    <body>
        <h1>YT Gemini Clipper</h1>
        <form method="post" action="/process">
            <label>Paste YouTube URL:</label><br/>
            <input type="text" name="url" placeholder="https://www.youtube.com/watch?v=..." required/>
            <select name="style">
                <option value="split">Split</option>
                <option value="left_focus">Left focus</option>
                <option value="right_focus">Right focus</option>
                <option value="all">All styles</option>
            </select>
            <button type="submit">Generate</button>
        </form>
        <hr/>
        <h2>Processed videos</h2>
    """

    for vid in video_ids:
        workdir = VIDEOS_DIR / vid
        highlights_path = workdir / "highlights.json"
        clips_dir = workdir / "clips"
        if not highlights_path.exists():
            continue

        try:
            data = json.loads(highlights_path.read_text(encoding="utf-8"))
        except Exception:
            continue

        html += f'<div class="video-card"><div class="title">Video ID: {vid}</div>'
        html += f'<div>Original URL: {data.get("video_url","")}</div>'

        html += '<div class="clip-list"><strong>Clips:</strong><br/>'
        if clips_dir.exists():
            for clip_file in sorted(clips_dir.glob("*.mp4")):
                html += f'<div class="clip-item"><a href="/download/{vid}/{clip_file.name}">{clip_file.name}</a></div>'
        else:
            html += "<div>No clips yet.</div>"
        html += "</div>"  # clip-list

        # For text for socials: show titles + descriptions from highlights
        html += "<div class='clip-list'><strong>Social text:</strong><br/>"
        for h in data.get("highlights", []):
            title = h.get("title", "")
            desc = h.get("description", "")
            html += f"<div class='clip-item'><div class='title'>{title}</div>"
            html += f"<div class='description'>{desc}</div></div>"
        html += "</div>"

        html += "</div>"  # video-card

    html += """
    </body>
    </html>
    """
    return HTMLResponse(html)


@app.post("/process")
async def process(url: str = Form(...), style: str = Form("split")):
    try:
        workdir = generate_highlights(url, style=style)
    except Exception as e:
        return HTMLResponse(f"<h1>Error</h1><pre>{e}</pre>", status_code=500)

    return RedirectResponse(url="/", status_code=303)


@app.get("/download/{video_id}/{filename}", response_class=FileResponse)
async def download_clip(video_id: str, filename: str):
    clip_path = VIDEOS_DIR / video_id / "clips" / filename
    if not clip_path.exists():
        return HTMLResponse("File not found", status_code=404)
    return FileResponse(clip_path, filename=filename, media_type="video/mp4")


if __name__ == "__main__":
    # If run directly: CLI behavior
    cli_main()
