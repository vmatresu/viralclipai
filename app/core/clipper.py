import subprocess
import logging
from pathlib import Path
from datetime import datetime, timedelta
from typing import Optional, Literal

logger = logging.getLogger(__name__)

AVAILABLE_STYLES = ["split", "left_focus", "right_focus"]
CROP_MODES = ["none", "center", "manual", "intelligent"]

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

def build_vf_filter(style: str, crop_mode: str = "none") -> str:
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

def run_ffmpeg_clip(
    start_str: str,
    end_str: str,
    out_path: Path,
    style: str,
    video_file: Path,
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
):
    try:
        t_start = parse_time(start_str)
        t_end = parse_time(end_str)

        if pad_before_seconds > 0:
            t_start = max(
                t_start - timedelta(seconds=pad_before_seconds),
                datetime(1900, 1, 1),
            )
        if pad_after_seconds > 0:
            t_end = t_end + timedelta(seconds=pad_after_seconds)

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
        
        # Generate thumbnail
        thumb_path = out_path.with_suffix(".jpg")
        cmd_thumb = [
            "ffmpeg", "-y",
            "-i", str(out_path),
            "-ss", "00:00:01",
            "-vframes", "1",
            "-vf", "scale=480:-2",
            str(thumb_path)
        ]
        try:
            subprocess.run(cmd_thumb, check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as e:
            logger.warning(f"Thumbnail generation failed for {out_path.name}: {e.stderr}")

    except subprocess.CalledProcessError as e:
        logger.error(f"ffmpeg failed:\n{e.stderr}")
        raise RuntimeError(f"FFmpeg clipping failed for {out_path.name}: {e.stderr}") from e


def run_intelligent_crop(
    video_file: Path,
    out_path: Path,
    start_str: str,
    end_str: str,
    target_aspect: str = "9:16",
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
) -> Path:
    """
    Run the intelligent cropping pipeline on a video segment.

    This function uses the smart_reframe module to analyze the video
    and generate an optimally cropped portrait/square version.

    Args:
        video_file: Path to the source video.
        out_path: Path for the output video.
        start_str: Start timestamp (HH:MM:SS or HH:MM:SS.fff).
        end_str: End timestamp (HH:MM:SS or HH:MM:SS.fff).
        target_aspect: Target aspect ratio (e.g., "9:16", "4:5", "1:1").
        pad_before_seconds: Seconds to add before start.
        pad_after_seconds: Seconds to add after end.

    Returns:
        Path to the rendered output video.
    """
    from app.core.smart_reframe import (
        Reframer,
        AspectRatio,
        IntelligentCropConfig,
    )

    try:
        # Parse timestamps
        t_start = parse_time(start_str)
        t_end = parse_time(end_str)

        if pad_before_seconds > 0:
            t_start = max(
                t_start - timedelta(seconds=pad_before_seconds),
                datetime(1900, 1, 1),
            )
        if pad_after_seconds > 0:
            t_end = t_end + timedelta(seconds=pad_after_seconds)

        start_seconds = (t_start - datetime(1900, 1, 1)).total_seconds()
        end_seconds = (t_end - datetime(1900, 1, 1)).total_seconds()

        # Parse aspect ratio
        aspect = AspectRatio.from_string(target_aspect)

        # Configure for reasonable speed on cloud VMs
        config = IntelligentCropConfig(
            fps_sample=3.0,
            analysis_resolution=480,
            render_preset="fast",
            render_crf=18,
        )

        # Create reframer
        reframer = Reframer(
            target_aspect_ratios=[aspect],
            config=config,
        )

        try:
            # Analyze and render
            output_prefix = str(out_path.with_suffix(""))
            output_paths = reframer.analyze_and_render(
                input_path=str(video_file),
                output_prefix=output_prefix,
                time_range=(start_seconds, end_seconds),
                save_crop_plan=False,
            )

            # Get the output path for our aspect ratio
            aspect_key = str(aspect)
            if aspect_key in output_paths:
                rendered_path = Path(output_paths[aspect_key])
                # Rename to expected output path if different
                if rendered_path != out_path:
                    rendered_path.rename(out_path)
                return out_path
            else:
                # Fallback - use whatever was produced
                for path in output_paths.values():
                    rendered_path = Path(path)
                    if rendered_path != out_path:
                        rendered_path.rename(out_path)
                    return out_path

            raise RuntimeError("No output produced by intelligent cropper")

        finally:
            reframer.close()

    except Exception as e:
        logger.error(f"Intelligent crop failed: {e}")
        raise RuntimeError(f"Intelligent crop failed for {out_path.name}: {e}") from e


def run_ffmpeg_clip_with_crop(
    start_str: str,
    end_str: str,
    out_path: Path,
    style: str,
    video_file: Path,
    crop_mode: Literal["none", "center", "manual", "intelligent"] = "none",
    target_aspect: str = "9:16",
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
):
    """
    Run FFmpeg clip with optional intelligent cropping.

    This is the main entry point that supports both traditional styles
    and the new intelligent crop mode.

    Args:
        start_str: Start timestamp.
        end_str: End timestamp.
        out_path: Output path for the clip.
        style: Style for traditional cropping (split, left_focus, right_focus).
        video_file: Path to source video.
        crop_mode: Cropping mode (none, center, manual, intelligent).
        target_aspect: Target aspect ratio for intelligent mode.
        pad_before_seconds: Seconds to pad before start.
        pad_after_seconds: Seconds to pad after end.
    """
    if crop_mode == "intelligent":
        run_intelligent_crop(
            video_file=video_file,
            out_path=out_path,
            start_str=start_str,
            end_str=end_str,
            target_aspect=target_aspect,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
        )
    else:
        # Use traditional clipping
        run_ffmpeg_clip(
            start_str=start_str,
            end_str=end_str,
            out_path=out_path,
            style=style,
            video_file=video_file,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
        )
