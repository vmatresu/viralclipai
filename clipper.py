import json
import subprocess
import argparse
from pathlib import Path
from datetime import datetime, timedelta

INPUT_JSON_NAME = "highlights.json"
VIDEO_FILE_NAME = "source.mp4"
CLIPS_DIR_NAME = "clips"

# Available styles:
AVAILABLE_STYLES = ["split", "left_focus", "right_focus"]


def ensure_dirs(workdir: Path) -> Path:
    clips_dir = workdir / CLIPS_DIR_NAME
    clips_dir.mkdir(parents=True, exist_ok=True)
    return clips_dir


def load_highlights(json_path: Path):
    with open(json_path, "r") as f:
        data = json.load(f)

    url = data.get("video_url")
    highlights = data.get("highlights", [])
    highlights = sorted(
        highlights,
        key=lambda h: (h.get("priority", 9999), h.get("id", 9999)),
    )
    return url, highlights


def download_video(url: str, video_file: Path):
    # Check if file exists and is large enough (heuristic for video content)
    if video_file.exists():
        if video_file.stat().st_size > 50 * 1024 * 1024:  # > 50MB
            print(f"[info] Using existing {video_file}")
            return
        print(f"[info] Existing file {video_file} seems too small (audio only?), re-downloading.")
        video_file.unlink(missing_ok=True)

    print(f"[info] Downloading video from {url}")
    try:
        subprocess.run(
            [
                "yt-dlp",
                "--remote-components", "ejs:github",
                "-f",
                "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
                "-o",
                str(video_file),
                url,
            ],
            check=True,
            capture_output=True,
            text=True
        )
    except subprocess.CalledProcessError as e:
        print(f"[error] yt-dlp failed:\n{e.stderr}")
        raise RuntimeError(f"Video download failed: {e.stderr}") from e


def parse_time(t_str: str) -> datetime:
    # Handle formats like "00:19:24.0" or "00:19:24"
    fmt = "%H:%M:%S.%f" if "." in t_str else "%H:%M:%S"
    return datetime.strptime(t_str, fmt)


def build_vf_filter(style: str) -> str:
    if style == "split":
        # Portrait 1080x1920: top = left half, bottom = right half
        return (
            "scale=1920:-2,split=2[full][full2];"
            "[full]crop=910:1080:0:0[left];"
            "[full2]crop=960:1080:960:0[right];"
            "[left]scale=1080:-2,crop=1080:960[left_scaled];"
            "[right]scale=1080:-2,crop=1080:960[right_scaled];"
            "[left_scaled][right_scaled]vstack=inputs=2"
        )

    if style == "left_focus":
        # Focus only on left half -> crop, then scale to 9:16
        return (
            "scale=1920:-2,"
            "crop=910:1080:0:0,"
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )

    if style == "right_focus":
        # Focus only on right half -> crop, then scale to 9:16
        return (
            "scale=1920:-2,"
            "crop=960:1080:960:0,"
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )

    # fallback: simple center crop
    return "scale=-2:1920,crop=1080:1920"


def run_ffmpeg_clip(start_str: str, end_str: str, out_path: Path, style: str, video_file: Path):
    # Padding logic
    try:
        t_start = parse_time(start_str)
        t_end = parse_time(end_str)

        # Pad -1s / +2s
        t_start = max(t_start - timedelta(seconds=1), datetime(1900, 1, 1))  # Clamp to 0
        t_end = t_end + timedelta(seconds=2)
        duration = (t_end - t_start).total_seconds()
        start_seconds = (t_start - datetime(1900, 1, 1)).total_seconds()
    except ValueError as e:
        print(f"[error] Time parsing failed: {e}")
        raise

    vf_filter = build_vf_filter(style)

    cmd = [
        "ffmpeg",
        "-y",
        "-ss",
        f"{start_seconds:.3f}",
        "-t",
        f"{duration:.3f}",
        "-i",
        str(video_file),
        "-vf",
        vf_filter,
        "-c:v",
        "libx264",
        "-preset",
        "fast",
        "-crf",
        "18",
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        str(out_path),
    ]

    try:
        subprocess.run(cmd, check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as e:
        print(f"[error] ffmpeg failed:\n{e.stderr}")
        raise RuntimeError(f"FFmpeg clipping failed for {out_path.name}: {e.stderr}") from e


def sanitize_filename(text: str, max_len: int = 60) -> str:
    text = text.lower().strip().replace(" ", "-")
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in text)
    return safe[:max_len] if safe else "clip"


def main():
    parser = argparse.ArgumentParser(description="Clip video from YouTube based on highlights.")
    parser.add_argument(
        "--workdir",
        required=True,
        help="Working directory for this video (e.g. ./videos/<youtube_id>)",
    )
    parser.add_argument(
        "--style",
        help="Output style.",
        default="split",
        choices=AVAILABLE_STYLES + ["all"],
    )

    args = parser.parse_args()
    workdir = Path(args.workdir).resolve()
    if not workdir.exists():
        raise SystemExit(f"[error] Workdir does not exist: {workdir}")

    json_path = workdir / INPUT_JSON_NAME
    if not json_path.exists():
        raise SystemExit(f"[error] highlights.json not found at {json_path}")

    clips_dir = ensure_dirs(workdir)
    video_file = workdir / VIDEO_FILE_NAME

    json_url, highlights = load_highlights(json_path)
    if not highlights:
        print("[error] No highlights found in JSON.")
        return

    video_url = json_url
    if not video_url:
        raise SystemExit("[error] 'video_url' missing in highlights.json.")

    download_video(video_url, video_file)

    if args.style == "all":
        styles_to_process = AVAILABLE_STYLES
    else:
        styles_to_process = [args.style]

    for h in highlights:
        clip_id = h.get("id")
        title = h.get("title", f"clip_{clip_id}")
        start = h["start"]
        end = h["end"]
        prio = h.get("priority", 99)
        safe_title = sanitize_filename(title)

        print(f"\n[clip] priority={prio} id={clip_id} | {title} | {start} -> {end}")

        for style in styles_to_process:
            filename = f"clip_{prio:02d}_{clip_id:02d}_{safe_title}_{style}.mp4"
            out_path = clips_dir / filename
            print(f" > Rendering style: {style} -> {out_path.name}")
            run_ffmpeg_clip(start, end, out_path, style=style, video_file=video_file)

    if video_file.exists():
        print(f"[cleanup] Removing source video {video_file}")
        video_file.unlink()

    print(f"\n[done] All clips rendered into {clips_dir}")


if __name__ == "__main__":
    main()
