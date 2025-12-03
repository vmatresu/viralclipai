import subprocess
import logging
from pathlib import Path
from datetime import datetime, timedelta
from typing import Optional, Literal

from app.core.utils.ffmpeg import run_ffmpeg

logger = logging.getLogger(__name__)

AVAILABLE_STYLES = ["split", "left_focus", "right_focus", "intelligent_split"]
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

def build_vf_filter(style: str, crop_mode: str = "none") -> Optional[str]:
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
    elif style == "original":
        return None
    elif style == "intelligent_split":
        # This style uses intelligent cropping, so no filter here
        # The intelligent cropping is handled separately
        return None
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
    ]
    
    if vf_filter:
        cmd.extend(["-vf", vf_filter])
        
    cmd.extend([
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "18",
        "-c:a", "aac",
        "-b:a", "128k",
        str(out_path),
    ])

    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
        
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
            run_ffmpeg(cmd_thumb, suppress_warnings=True, log_level="error", check=True)
        except RuntimeError as e:
            logger.warning(f"Thumbnail generation failed for {out_path.name}: {e}")

    except RuntimeError as e:
        logger.error(f"ffmpeg failed: {e}")
        raise RuntimeError(f"FFmpeg clipping failed for {out_path.name}: {e}") from e


def run_intelligent_crop(
    video_file: Path,
    out_path: Path,
    start_str: str,
    end_str: str,
    target_aspect: str = "9:16",
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
    shot_cache=None,
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
        shot_cache: Optional shot detection cache for performance.

    Returns:
        Path to the rendered output video.
    """
    from app.core.smart_reframe import (
        Reframer,
        AspectRatio,
    )
    from app.core.smart_reframe.config_factory import get_production_config

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

        # Use optimized production configuration
        config = get_production_config()

        # Create reframer with cache support
        reframer = Reframer(
            target_aspect_ratios=[aspect],
            config=config,
            shot_cache=shot_cache,
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


def run_intelligent_split_crop(
    video_file: Path,
    out_path: Path,
    start_str: str,
    end_str: str,
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
    shot_cache=None,
) -> Path:
    """
    Run intelligent cropping on both left and right halves and stack them.
    Face tracks left half and puts on top, face tracks right half and puts on bottom.
    """
    import tempfile
    import shutil

    logger.info(f"Running Intelligent Split for {out_path.name}")
    
    # Create temp dir for intermediate files
    temp_dir = Path(tempfile.mkdtemp())
    try:
        # 1. Cut the time segment first (source segment)
        # This avoids analyzing the whole video or complex offset math
        segment_path = temp_dir / "segment.mp4"
        run_ffmpeg_clip(
            start_str=start_str,
            end_str=end_str,
            out_path=segment_path,
            style="original", # No crop, just trim
            video_file=video_file,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds
        )

        # 2. Extract Left and Right halves
        left_half = temp_dir / "left.mp4"
        right_half = temp_dir / "right.mp4"
        
        # Left: crop width/2 from 0,0
        cmd_left = [
            "ffmpeg", "-y", "-i", str(segment_path),
            "-vf", "crop=iw/2:ih:0:0",
            "-c:v", "libx264", "-preset", "fast", "-c:a", "copy",
            str(left_half)
        ]
        # Right: crop width/2 from width/2,0
        cmd_right = [
            "ffmpeg", "-y", "-i", str(segment_path),
            "-vf", "crop=iw/2:ih:iw/2:0",
            "-c:v", "libx264", "-preset", "fast", "-c:a", "copy",
            str(right_half)
        ]
        
        subprocess.run(cmd_left, check=True, capture_output=True)
        subprocess.run(cmd_right, check=True, capture_output=True)

        # 3. Run Intelligent Crop (face tracking) on each half
        # Helper to get duration
        def get_duration(p: Path) -> float:
            import json
            res = subprocess.run(
                ["ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", str(p)],
                capture_output=True, text=True
            )
            data = json.loads(res.stdout)
            return float(data["format"]["duration"])

        dur = get_duration(segment_path)
        end_time_str = str(timedelta(seconds=dur))
        
        left_cropped = temp_dir / "left_crop.mp4"
        right_cropped = temp_dir / "right_crop.mp4"
        
        # Face track left half and crop to 9:8 aspect (for top)
        run_intelligent_crop(
            left_half, 
            left_cropped, 
            "00:00:00", 
            end_time_str, 
            target_aspect="9:8",
            shot_cache=shot_cache
        )
        # Face track right half and crop to 9:8 aspect (for bottom)
        run_intelligent_crop(
            right_half, 
            right_cropped, 
            "00:00:00", 
            end_time_str, 
            target_aspect="9:8",
            shot_cache=shot_cache
        )

        # 4. Stack them (left on top, right on bottom)
        cmd_stack = [
            "ffmpeg", "-y",
            "-i", str(left_cropped),
            "-i", str(right_cropped),
            "-filter_complex",
            "[0:v]scale=1080:960:force_original_aspect_ratio=decrease,pad=1080:960:(ow-iw)/2:(oh-ih)/2[top];"
            "[1:v]scale=1080:960:force_original_aspect_ratio=decrease,pad=1080:960:(ow-iw)/2:(oh-ih)/2[bottom];"
            "[top][bottom]vstack",
            "-c:v", "libx264", "-preset", "fast", "-crf", "18",
            "-c:a", "aac", "-b:a", "128k",
            str(out_path)
        ]
        
        subprocess.run(cmd_stack, check=True, capture_output=True, text=True)
        
        # Thumbnail
        thumb_path = out_path.with_suffix(".jpg")
        cmd_thumb = [
            "ffmpeg", "-y",
            "-i", str(out_path),
            "-ss", "00:00:01",
            "-vframes", "1",
            "-vf", "scale=480:-2",
            str(thumb_path)
        ]
        subprocess.run(cmd_thumb, check=True, capture_output=True)

    except Exception as e:
        logger.error(f"Smart split failed: {e}")
        raise RuntimeError(f"Smart split failed: {e}") from e
    finally:
        shutil.rmtree(temp_dir)
    
    return out_path


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
    shot_cache=None,
):
    """
    Run FFmpeg clip with optional intelligent cropping.

    This is the main entry point that supports both traditional styles
    and the new intelligent crop mode.

    Args:
        start_str: Start timestamp.
        end_str: End timestamp.
        out_path: Output path for the clip.
        style: Style for traditional cropping (split, left_focus, right_focus, original).
        video_file: Path to source video.
        crop_mode: Cropping mode (none, center, manual, intelligent).
        target_aspect: Target aspect ratio for intelligent mode.
        pad_before_seconds: Seconds to pad before start.
        pad_after_seconds: Seconds to pad after end.
        shot_cache: Optional shot detection cache for performance optimization.
    """
    # "original" style always preserves original format, regardless of crop_mode
    if style == "original":
        run_ffmpeg_clip(
            start_str=start_str,
            end_str=end_str,
            out_path=out_path,
            style=style,
            video_file=video_file,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
        )
    # "intelligent_split" style uses intelligent cropping with face tracking for split view
    elif style == "intelligent_split":
        run_intelligent_split_crop(
            video_file=video_file,
            out_path=out_path,
            start_str=start_str,
            end_str=end_str,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
            shot_cache=shot_cache,
        )
    elif crop_mode == "intelligent":
        run_intelligent_crop(
            video_file=video_file,
            out_path=out_path,
            start_str=start_str,
            end_str=end_str,
            target_aspect=target_aspect,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
            shot_cache=shot_cache,
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
