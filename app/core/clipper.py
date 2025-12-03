"""
Video clipping and processing module.

This module provides functions for clipping videos with various styles and
intelligent cropping capabilities. It follows SOLID principles and production
best practices for maintainability and performance.
"""

import json
import logging
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Literal, Optional

from app.core.smart_reframe import AspectRatio, Reframer
from app.core.smart_reframe.config_factory import get_production_config
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.utils.ffmpeg import run_ffmpeg

logger = logging.getLogger(__name__)

# ============================================================================
# Constants
# ============================================================================

AVAILABLE_STYLES = ["split", "left_focus", "right_focus", "intelligent_split"]
CROP_MODES = ["none", "center", "manual", "intelligent"]

# Video encoding constants
DEFAULT_AUDIO_BITRATE = "128k"
THUMBNAIL_SCALE_WIDTH = 480
THUMBNAIL_TIMESTAMP = "00:00:01"

# Split view constants
SPLIT_VIEW_TOP_RESOLUTION = (1080, 960)
SPLIT_VIEW_BOTTOM_RESOLUTION = (1080, 960)
SPLIT_VIEW_TARGET_ASPECT = "9:8"

# Default encoding settings (fallback when config not available)
DEFAULT_VIDEO_CODEC = "libx264"
DEFAULT_AUDIO_CODEC = "aac"
DEFAULT_PRESET = "fast"
DEFAULT_CRF = 18  # Used for traditional styles, intelligent uses config

# Minimum video file size threshold (50MB)
MIN_VIDEO_FILE_SIZE = 50 * 1024 * 1024


# ============================================================================
# Configuration Helpers
# ============================================================================

@dataclass
class EncodingConfig:
    """Video encoding configuration."""
    codec: str = DEFAULT_VIDEO_CODEC
    preset: str = DEFAULT_PRESET
    crf: int = DEFAULT_CRF
    audio_codec: str = DEFAULT_AUDIO_CODEC
    audio_bitrate: str = DEFAULT_AUDIO_BITRATE

    @classmethod
    def from_intelligent_config(cls, config: IntelligentCropConfig) -> "EncodingConfig":
        """Create encoding config from intelligent crop config."""
        return cls(
            codec=DEFAULT_VIDEO_CODEC,
            preset=config.render_preset,
            crf=config.render_crf,
            audio_codec=DEFAULT_AUDIO_CODEC,
            audio_bitrate=DEFAULT_AUDIO_BITRATE,
        )

    def to_ffmpeg_args(self) -> list[str]:
        """Convert to FFmpeg command arguments."""
        return [
            "-c:v", self.codec,
            "-preset", self.preset,
            "-crf", str(self.crf),
            "-c:a", self.audio_codec,
            "-b:a", self.audio_bitrate,
        ]
    
    def with_crf(self, crf: int) -> "EncodingConfig":
        """Return a new config with updated CRF."""
        return EncodingConfig(
            codec=self.codec,
            preset=self.preset,
            crf=crf,
            audio_codec=self.audio_codec,
            audio_bitrate=self.audio_bitrate,
        )

# ============================================================================
# Utility Functions
# ============================================================================

def ensure_dirs(workdir: Path) -> Path:
    """Ensure clips directory exists."""
    clips_dir = workdir / "clips"
    clips_dir.mkdir(parents=True, exist_ok=True)
    return clips_dir


def get_video_duration(video_path: Path) -> float:
    """
    Get video duration in seconds using ffprobe.
    
    Args:
        video_path: Path to video file.
        
    Returns:
        Duration in seconds.
        
    Raises:
        RuntimeError: If ffprobe fails or duration cannot be determined.
    """
    try:
        result = subprocess.run(
            [
                "ffprobe",
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                str(video_path),
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        data = json.loads(result.stdout)
        return float(data["format"]["duration"])
    except (subprocess.CalledProcessError, json.JSONDecodeError, KeyError, ValueError) as e:
        logger.error(f"Failed to get video duration for {video_path}: {e}")
        raise RuntimeError(f"Failed to get video duration: {e}") from e


def generate_thumbnail(video_path: Path, output_path: Path) -> None:
    """
    Generate thumbnail from video.
    
    Args:
        video_path: Path to source video.
        output_path: Path for thumbnail output.
    """
    cmd = [
        "ffmpeg", "-y",
        "-i", str(video_path),
        "-ss", THUMBNAIL_TIMESTAMP,
        "-vframes", "1",
        "-vf", f"scale={THUMBNAIL_SCALE_WIDTH}:-2",
        str(output_path),
    ]
    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
    except RuntimeError as e:
        logger.warning(f"Thumbnail generation failed for {video_path.name}: {e}")

def download_video(url: str, video_file: Path) -> None:
    """
    Download video from URL using yt-dlp.
    
    Args:
        url: Video URL to download.
        video_file: Path where video will be saved.
        
    Raises:
        RuntimeError: If download fails.
    """
    if video_file.exists():
        if video_file.stat().st_size > MIN_VIDEO_FILE_SIZE:
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
    """
    Parse time string to datetime object.
    
    Args:
        t_str: Time string in format HH:MM:SS or HH:MM:SS.fff.
        
    Returns:
        Datetime object.
        
    Raises:
        ValueError: If time string format is invalid.
    """
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
        
    # Use default encoding config for traditional styles
    encoding_config = EncodingConfig()
    cmd.extend(encoding_config.to_ffmpeg_args())
    cmd.append(str(out_path))

    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
        
        # Generate thumbnail
        thumb_path = out_path.with_suffix(".jpg")
        generate_thumbnail(out_path, thumb_path)

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
    shot_cache: Optional[Any] = None,
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
        
    Raises:
        RuntimeError: If intelligent cropping fails.
    """

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


# ============================================================================
# Split View Processing Functions
# ============================================================================

def _extract_video_halves(
    video_path: Path,
    start_str: str,
    end_str: str,
    left_output: Path,
    right_output: Path,
    encoding_config: EncodingConfig,
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
) -> None:
    """
    Extract left and right halves from a video segment directly from source.
    
    Args:
        video_path: Path to source video.
        start_str: Start timestamp.
        end_str: End timestamp.
        left_output: Path for left half output.
        right_output: Path for right half output.
        encoding_config: Encoding configuration to use.
        pad_before_seconds: Seconds to add before start.
        pad_after_seconds: Seconds to add after end.
        
    Raises:
        RuntimeError: If extraction fails.
    """
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

    # Left half: crop width/2 from 0,0
    cmd_left = [
        "ffmpeg", "-y",
        "-ss", f"{start_seconds:.3f}",
        "-t", f"{duration:.3f}",
        "-i", str(video_path),
        "-vf", "crop=iw/2:ih:0:0",
    ] + encoding_config.to_ffmpeg_args() + [
        "-c:a", "copy",  # Copy audio without re-encoding
        str(left_output),
    ]
    
    # Right half: crop width/2 from width/2,0
    cmd_right = [
        "ffmpeg", "-y",
        "-ss", f"{start_seconds:.3f}",
        "-t", f"{duration:.3f}",
        "-i", str(video_path),
        "-vf", "crop=iw/2:ih:iw/2:0",
    ] + encoding_config.to_ffmpeg_args() + [
        "-c:a", "copy",  # Copy audio without re-encoding
        str(right_output),
    ]
    
    try:
        run_ffmpeg(cmd_left, suppress_warnings=True, log_level="error", check=True)
        run_ffmpeg(cmd_right, suppress_warnings=True, log_level="error", check=True)
    except RuntimeError as e:
        logger.error(f"Failed to extract video halves: {e}")
        raise RuntimeError(f"Video half extraction failed: {e}") from e


def _stack_split_view_videos(
    top_video: Path,
    bottom_video: Path,
    output_path: Path,
    encoding_config: EncodingConfig,
) -> None:
    """
    Stack two videos vertically (top and bottom).
    
    Args:
        top_video: Path to top video (left half).
        bottom_video: Path to bottom video (right half).
        output_path: Path for stacked output.
        encoding_config: Encoding configuration to use.
        
    Raises:
        RuntimeError: If stacking fails.
    """
    top_w, top_h = SPLIT_VIEW_TOP_RESOLUTION
    bottom_w, bottom_h = SPLIT_VIEW_BOTTOM_RESOLUTION
    
    filter_complex = (
        f"[0:v]scale={top_w}:{top_h}:force_original_aspect_ratio=decrease,"
        f"pad={top_w}:{top_h}:(ow-iw)/2:(oh-ih)/2[top];"
        f"[1:v]scale={bottom_w}:{bottom_h}:force_original_aspect_ratio=decrease,"
        f"pad={bottom_w}:{bottom_h}:(ow-iw)/2:(oh-ih)/2[bottom];"
        "[top][bottom]vstack"
    )
    
    cmd = [
        "ffmpeg", "-y",
        "-i", str(top_video),
        "-i", str(bottom_video),
        "-filter_complex", filter_complex,
    ] + encoding_config.to_ffmpeg_args() + [
        str(output_path),
    ]
    
    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
    except RuntimeError as e:
        logger.error(f"Failed to stack split view videos: {e}")
        raise RuntimeError(f"Video stacking failed: {e}") from e


def run_intelligent_split_crop(
    video_file: Path,
    out_path: Path,
    start_str: str,
    end_str: str,
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
    shot_cache: Optional[Any] = None,
) -> Path:
    """
    Run intelligent cropping on both left and right halves and stack them.
    
    This function processes a video by:
    1. Extracting the left and right halves directly from source
    2. Applying intelligent face-tracking crop to each half
    3. Stacking the halves vertically (left on top, right on bottom)
    
    Args:
        video_file: Path to source video.
        out_path: Path for output video.
        start_str: Start timestamp (HH:MM:SS or HH:MM:SS.fff).
        end_str: End timestamp (HH:MM:SS or HH:MM:SS.fff).
        pad_before_seconds: Seconds to add before start.
        pad_after_seconds: Seconds to add after end.
        shot_cache: Optional shot detection cache for performance.
        
    Returns:
        Path to the rendered output video.
        
    Raises:
        RuntimeError: If processing fails at any stage.
    """
    logger.info(f"Running Intelligent Split for {out_path.name}")
    
    # Get production encoding config for consistency
    intelligent_config = get_production_config()
    encoding_config = EncodingConfig.from_intelligent_config(intelligent_config)
    
    # Create temp dir for intermediate files
    temp_dir = Path(tempfile.mkdtemp())
    try:
        # Step 1: Extract left and right halves directly from source
        # This replaces the previous 2-step process (Segment -> Split)
        left_half = temp_dir / "left.mp4"
        right_half = temp_dir / "right.mp4"
        
        _extract_video_halves(
            video_path=video_file,
            start_str=start_str,
            end_str=end_str,
            left_output=left_half,
            right_output=right_half,
            encoding_config=encoding_config,
            pad_before_seconds=pad_before_seconds,
            pad_after_seconds=pad_after_seconds,
        )

        # Step 2: Run intelligent crop on each half
        # We use the full duration since the halves are already trimmed
        duration = get_video_duration(left_half)
        end_time_str = str(timedelta(seconds=duration))
        
        left_cropped = temp_dir / "left_crop.mp4"
        right_cropped = temp_dir / "right_crop.mp4"
        
        # Face track and crop each half to 9:8 aspect
        run_intelligent_crop(
            left_half,
            left_cropped,
            "00:00:00",
            end_time_str,
            target_aspect=SPLIT_VIEW_TARGET_ASPECT,
            shot_cache=shot_cache,
        )
        run_intelligent_crop(
            right_half,
            right_cropped,
            "00:00:00",
            end_time_str,
            target_aspect=SPLIT_VIEW_TARGET_ASPECT,
            shot_cache=shot_cache,
        )

        # Step 3: Stack halves vertically
        # Optimization: Increase CRF for final stacking to reduce file size
        # Split views have high visual complexity, so we increase CRF (lower quality)
        # slightly to avoid file size bloat vs single view clips.
        # +4 CRF roughly halves the bitrate/filesize
        final_encoding_config = encoding_config.with_crf(encoding_config.crf + 4)
        
        _stack_split_view_videos(left_cropped, right_cropped, out_path, final_encoding_config)
        
        # Step 4: Generate thumbnail
        thumb_path = out_path.with_suffix(".jpg")
        generate_thumbnail(out_path, thumb_path)

    except Exception as e:
        logger.error(f"Intelligent split crop failed for {out_path.name}: {e}")
        raise RuntimeError(f"Intelligent split crop failed: {e}") from e
    finally:
        # Always cleanup temp directory
        if temp_dir.exists():
            shutil.rmtree(temp_dir, ignore_errors=True)
    
    return out_path


def extract_segment(
    video_path: Path,
    start_str: str,
    end_str: str,
    out_path: Path,
    pad_before_seconds: float = 0.0,
    pad_after_seconds: float = 0.0,
) -> None:
    """
    Extract a high-quality intermediate segment from the source video.
    
    Used to create smaller working files for each scene, allowing the
    large original video to be deleted early.
    
    Args:
        video_path: Path to source video.
        start_str: Start timestamp.
        end_str: End timestamp.
        out_path: Output path for the segment.
        pad_before_seconds: Seconds to include before start.
        pad_after_seconds: Seconds to include after end.
        
    Raises:
        RuntimeError: If extraction fails.
    """
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

    # Use high quality settings for intermediate segment
    # CRF 18 is visually lossless for most purposes
    # Preset 'superfast' speeds up extraction significantly with minimal quality loss at this CRF
    cmd = [
        "ffmpeg", "-y",
        "-ss", f"{start_seconds:.3f}",
        "-t", f"{duration:.3f}",
        "-i", str(video_path),
        "-c:v", "libx264",
        "-preset", "superfast",
        "-crf", "18",
        "-c:a", "aac",
        "-b:a", "192k",
        str(out_path),
    ]

    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
    except RuntimeError as e:
        logger.error(f"Segment extraction failed: {e}")
        raise RuntimeError(f"Segment extraction failed for {out_path.name}: {e}") from e


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
