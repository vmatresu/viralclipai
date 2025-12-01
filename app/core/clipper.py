import subprocess
import logging
from pathlib import Path
from datetime import datetime, timedelta

logger = logging.getLogger(__name__)

AVAILABLE_STYLES = ["split", "left_focus", "right_focus"]

def ensure_dirs(workdir: Path) -> Path:
    clips_dir = workdir / "clips"
    clips_dir.mkdir(parents=True, exist_ok=True)
    return clips_dir

def download_video(url: str, video_file: Path):
    if video_file.exists():
        if video_file.stat().st_size > 50 * 1024 * 1024:
            logger.info(f"Using existing {video_file}")
            return
        logger.info(f"Existing file {video_file} seems too small, re-downloading.")
        video_file.unlink(missing_ok=True)

    logger.info(f"Downloading video from {url}")
    try:
        subprocess.run(
            [
                "yt-dlp",
                "--remote-components", "ejs:github",
                "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
                "-o", str(video_file),
                url,
            ],
            check=True,
            capture_output=True,
            text=True
        )
    except subprocess.CalledProcessError as e:
        logger.error(f"yt-dlp failed:\n{e.stderr}")
        raise RuntimeError(f"Video download failed: {e.stderr}") from e

def parse_time(t_str: str) -> datetime:
    fmt = "%H:%M:%S.%f" if "." in t_str else "%H:%M:%S"
    return datetime.strptime(t_str, fmt)

def build_vf_filter(style: str) -> str:
    if style == "split":
        return (
            "scale=1920:-2,split=2[full][full2];"
            "[full]crop=910:1080:0:0[left];"
            "[full2]crop=960:1080:960:0[right];"
            "[left]scale=1080:-2,crop=1080:960[left_scaled];"
            "[right]scale=1080:-2,crop=1080:960[right_scaled];"
            "[left_scaled][right_scaled]vstack=inputs=2"
        )
    elif style == "left_focus":
        return (
            "scale=1920:-2,"
            "crop=910:1080:0:0,"
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )
    elif style == "right_focus":
        return (
            "scale=1920:-2,"
            "crop=960:1080:960:0,"
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )
    return "scale=-2:1920,crop=1080:1920"

def run_ffmpeg_clip(start_str: str, end_str: str, out_path: Path, style: str, video_file: Path):
    try:
        t_start = parse_time(start_str)
        t_end = parse_time(end_str)
        
        # Pad -1s / +2s
        t_start = max(t_start - timedelta(seconds=1), datetime(1900, 1, 1))
        t_end = t_end + timedelta(seconds=2)
        duration = (t_end - t_start).total_seconds()
        start_seconds = (t_start - datetime(1900, 1, 1)).total_seconds()
    except ValueError as e:
        logger.error(f"Time parsing failed: {e}")
        raise

    vf_filter = build_vf_filter(style)

    cmd = [
        "ffmpeg",
        "-y",
        "-ss", f"{start_seconds:.3f}",
        "-t", f"{duration:.3f}",
        "-i", str(video_file),
        "-vf", vf_filter,
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "18",
        "-c:a", "aac",
        "-b:a", "128k",
        str(out_path),
    ]

    try:
        subprocess.run(cmd, check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as e:
        logger.error(f"ffmpeg failed:\n{e.stderr}")
        raise RuntimeError(f"FFmpeg clipping failed for {out_path.name}: {e.stderr}") from e
