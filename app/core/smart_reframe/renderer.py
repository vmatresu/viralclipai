"""
FFmpeg-based rendering of reframed videos.

This module converts crop plans into FFmpeg commands
to produce the final reframed video files.
"""

import json
import logging
import subprocess
import tempfile
from pathlib import Path
from typing import Optional

from app.core.smart_reframe.models import (
    AspectRatio,
    CropPlan,
    CropWindow,
    ShotCropPlan,
)
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.smart_reframe.crop_planner import interpolate_crop_window

logger = logging.getLogger(__name__)


class Renderer:
    """
    Render reframed videos using FFmpeg.
    """

    def __init__(self, config: IntelligentCropConfig):
        self.config = config

    def render(
        self,
        input_path: str,
        crop_plan: CropPlan,
        output_prefix: str,
        output_resolution: Optional[tuple[int, int]] = None,
    ) -> dict[str, str]:
        """
        Render reframed videos for all target aspect ratios.

        Args:
            input_path: Path to source video.
            crop_plan: Computed crop plan.
            output_prefix: Prefix for output file names.
            output_resolution: Optional (width, height) for output.
                              If not provided, uses crop dimensions.

        Returns:
            Dict mapping aspect ratio string to output path.
        """
        output_paths = {}

        for aspect_ratio in crop_plan.target_aspect_ratios:
            # Collect crop plans for this aspect ratio
            aspect_crop_plans = [
                cp
                for cp in crop_plan.shot_crop_plans
                if cp.aspect_ratio == aspect_ratio
            ]

            if not aspect_crop_plans:
                logger.warning(f"No crop plans for aspect ratio {aspect_ratio}")
                continue

            # Generate output path
            aspect_str = f"{aspect_ratio.width}x{aspect_ratio.height}"
            output_path = f"{output_prefix}_{aspect_str}.mp4"

            # Determine rendering strategy
            if self._is_static_crop(aspect_crop_plans):
                self._render_static_crop(
                    input_path,
                    aspect_crop_plans,
                    output_path,
                    output_resolution,
                    crop_plan,
                )
            else:
                self._render_dynamic_crop(
                    input_path,
                    aspect_crop_plans,
                    output_path,
                    output_resolution,
                    crop_plan,
                )

            output_paths[str(aspect_ratio)] = output_path
            logger.info(f"Rendered {output_path}")

        return output_paths

    def _get_time_range(self, crop_plan: CropPlan) -> tuple[float, float]:
        """
        Get the overall time range from the crop plan's shots.
        """
        if not crop_plan.shots:
            return 0.0, crop_plan.video.duration

        start_time = min(shot.start_time for shot in crop_plan.shots)
        end_time = max(shot.end_time for shot in crop_plan.shots)
        return start_time, end_time

    def _is_static_crop(self, crop_plans: list[ShotCropPlan]) -> bool:
        """
        Check if all crop windows are essentially static.

        Returns True if a simple static crop can be used.
        """
        all_windows = []
        for cp in crop_plans:
            all_windows.extend(cp.crop_windows)

        if len(all_windows) <= 1:
            return True

        # Check variance in crop windows
        x_vals = [w.x for w in all_windows]
        y_vals = [w.y for w in all_windows]
        w_vals = [w.width for w in all_windows]

        x_range = max(x_vals) - min(x_vals)
        y_range = max(y_vals) - min(y_vals)
        w_range = max(w_vals) - min(w_vals)

        avg_width = sum(w_vals) / len(w_vals)

        # Consider static if movement is less than 5% of width
        threshold = avg_width * 0.05
        return x_range < threshold and y_range < threshold and w_range < threshold

    def _render_static_crop(
        self,
        input_path: str,
        crop_plans: list[ShotCropPlan],
        output_path: str,
        output_resolution: Optional[tuple[int, int]],
        full_plan: CropPlan,
    ):
        """
        Render using a single static crop (faster).
        """
        # Use the median crop window
        all_windows = []
        for cp in crop_plans:
            all_windows.extend(cp.crop_windows)

        if not all_windows:
            raise ValueError("No crop windows to render")

        # Compute median crop
        x = int(sorted(w.x for w in all_windows)[len(all_windows) // 2])
        y = int(sorted(w.y for w in all_windows)[len(all_windows) // 2])
        width = int(sorted(w.width for w in all_windows)[len(all_windows) // 2])
        height = int(sorted(w.height for w in all_windows)[len(all_windows) // 2])

        # Get time range from shots
        start_time, end_time = self._get_time_range(full_plan)

        # Build FFmpeg command
        vf_filters = [f"crop={width}:{height}:{x}:{y}"]

        if output_resolution:
            vf_filters.append(f"scale={output_resolution[0]}:{output_resolution[1]}")
        else:
            # Scale to even dimensions (required by many codecs)
            vf_filters.append("scale=trunc(iw/2)*2:trunc(ih/2)*2")

        vf = ",".join(vf_filters)

        cmd = [
            "ffmpeg",
            "-y",
            "-ss", f"{start_time:.3f}",
            "-i", input_path,
            "-t", f"{end_time - start_time:.3f}",
            "-vf", vf,
            "-c:v", "libx264",
            "-preset", self.config.render_preset,
            "-crf", str(self.config.render_crf),
            "-c:a", "aac",
            "-b:a", "128k",
            output_path,
        ]

        self._run_ffmpeg(cmd)

    def _render_dynamic_crop(
        self,
        input_path: str,
        crop_plans: list[ShotCropPlan],
        output_path: str,
        output_resolution: Optional[tuple[int, int]],
        full_plan: CropPlan,
    ):
        """
        Render using dynamic crop expressions or segment concatenation.

        This handles time-varying crops by generating FFmpeg filter
        expressions or splitting into segments.
        """
        # Sort shots by time
        shots_sorted = sorted(full_plan.shots, key=lambda s: s.start_time)

        # Check if we can use expression-based cropping
        # For simplicity, we use segment-based approach
        self._render_segments(
            input_path,
            crop_plans,
            output_path,
            output_resolution,
            full_plan,
        )

    def _render_segments(
        self,
        input_path: str,
        crop_plans: list[ShotCropPlan],
        output_path: str,
        output_resolution: Optional[tuple[int, int]],
        full_plan: CropPlan,
    ):
        """
        Render by splitting into segments with constant crops.
        """
        # Merge all crop windows sorted by time
        all_windows: list[tuple[float, CropWindow]] = []
        for cp in crop_plans:
            for cw in cp.crop_windows:
                all_windows.append((cw.time, cw))

        all_windows.sort(key=lambda x: x[0])

        if not all_windows:
            raise ValueError("No crop windows to render")

        # Group into segments with similar crops
        segments = self._group_segments(all_windows)

        if len(segments) == 1:
            # Single segment - use direct crop
            seg = segments[0]
            self._render_single_segment(
                input_path,
                seg["start"],
                seg["end"],
                seg["crop"],
                output_path,
                output_resolution,
            )
        else:
            # Multiple segments - concatenate
            self._render_and_concat_segments(
                input_path,
                segments,
                output_path,
                output_resolution,
            )

    def _group_segments(
        self,
        windows: list[tuple[float, CropWindow]],
        tolerance: float = 0.1,
    ) -> list[dict]:
        """
        Group crop windows into segments with similar crops.
        """
        if not windows:
            return []

        segments = []
        current_start = windows[0][0]
        current_crop = windows[0][1]
        prev_time = windows[0][0]

        for time, crop in windows[1:]:
            # Check if crop has changed significantly
            if self._crops_differ(current_crop, crop, tolerance):
                # End current segment
                segments.append(
                    {
                        "start": current_start,
                        "end": time,
                        "crop": current_crop,
                    }
                )
                current_start = time
                current_crop = crop

            prev_time = time

        # Add final segment
        segments.append(
            {
                "start": current_start,
                "end": prev_time + 1.0,  # Extend slightly
                "crop": current_crop,
            }
        )

        return segments

    def _crops_differ(
        self,
        crop1: CropWindow,
        crop2: CropWindow,
        tolerance: float,
    ) -> bool:
        """Check if two crops are significantly different."""
        threshold = crop1.width * tolerance
        return (
            abs(crop1.x - crop2.x) > threshold
            or abs(crop1.y - crop2.y) > threshold
            or abs(crop1.width - crop2.width) > threshold
        )

    def _render_single_segment(
        self,
        input_path: str,
        start: float,
        end: float,
        crop: CropWindow,
        output_path: str,
        output_resolution: Optional[tuple[int, int]],
    ):
        """Render a single segment with constant crop."""
        duration = end - start

        vf_filters = [f"crop={crop.width}:{crop.height}:{crop.x}:{crop.y}"]

        if output_resolution:
            vf_filters.append(f"scale={output_resolution[0]}:{output_resolution[1]}")
        else:
            vf_filters.append("scale=trunc(iw/2)*2:trunc(ih/2)*2")

        vf = ",".join(vf_filters)

        cmd = [
            "ffmpeg",
            "-y",
            "-ss", f"{start:.3f}",
            "-i", input_path,
            "-t", f"{duration:.3f}",
            "-vf", vf,
            "-c:v", "libx264",
            "-preset", self.config.render_preset,
            "-crf", str(self.config.render_crf),
            "-c:a", "aac",
            "-b:a", "128k",
            output_path,
        ]

        self._run_ffmpeg(cmd)

    def _render_and_concat_segments(
        self,
        input_path: str,
        segments: list[dict],
        output_path: str,
        output_resolution: Optional[tuple[int, int]],
    ):
        """Render segments and concatenate them."""
        with tempfile.TemporaryDirectory() as tmpdir:
            segment_files = []

            for i, seg in enumerate(segments):
                seg_path = Path(tmpdir) / f"segment_{i:04d}.mp4"

                self._render_single_segment(
                    input_path,
                    seg["start"],
                    seg["end"],
                    seg["crop"],
                    str(seg_path),
                    output_resolution,
                )

                segment_files.append(seg_path)

            # Create concat list
            concat_list = Path(tmpdir) / "concat.txt"
            with open(concat_list, "w") as f:
                for seg_path in segment_files:
                    f.write(f"file '{seg_path}'\n")

            # Concatenate
            cmd = [
                "ffmpeg",
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                str(concat_list),
                "-c",
                "copy",
                output_path,
            ]

            self._run_ffmpeg(cmd)

    def _run_ffmpeg(self, cmd: list[str]):
        """Run an FFmpeg command with improved error handling."""
        from app.core.utils.ffmpeg import run_ffmpeg
        
        # Note: run_ffmpeg will modify cmd to add -loglevel, so log after calling
        logger.debug(f"Running FFmpeg command (log level will be added automatically)")

        try:
            result = run_ffmpeg(
                cmd,
                suppress_warnings=True,
                log_level="error",  # Suppress warnings, keep errors
                check=True,
            )
            if result.stdout:
                logger.debug(f"FFmpeg stdout: {result.stdout}")
        except RuntimeError as e:
            # Re-raise as-is (already formatted)
            raise
        except Exception as e:
            logger.error(f"FFmpeg failed: {e}")
            raise RuntimeError(f"FFmpeg rendering failed: {e}") from e


def render_with_letterbox(
    input_path: str,
    crop_window: CropWindow,
    output_path: str,
    target_resolution: tuple[int, int],
    blur_sigma: float = 30.0,
    config: Optional[IntelligentCropConfig] = None,
):
    """
    Render with blurred letterbox background instead of black bars.

    Args:
        input_path: Path to source video.
        crop_window: Crop window to use.
        output_path: Path for output video.
        target_resolution: (width, height) of output.
        blur_sigma: Blur strength for background.
        config: Optional configuration.
    """
    if config is None:
        config = IntelligentCropConfig()

    out_w, out_h = target_resolution
    crop = crop_window

    # Build complex filter for blurred background + sharp foreground
    # 1. Scale input to output size with blur for background
    # 2. Crop and scale for foreground
    # 3. Overlay foreground on background
    vf = (
        f"split=2[bg][fg];"
        f"[bg]scale={out_w}:{out_h}:force_original_aspect_ratio=increase,"
        f"crop={out_w}:{out_h},gblur=sigma={blur_sigma}[blurred];"
        f"[fg]crop={crop.width}:{crop.height}:{crop.x}:{crop.y},"
        f"scale={out_w}:{out_h}:force_original_aspect_ratio=decrease[cropped];"
        f"[blurred][cropped]overlay=(W-w)/2:(H-h)/2"
    )

    cmd = [
        "ffmpeg",
        "-y",
        "-i",
        input_path,
        "-vf",
        vf,
        "-c:v",
        "libx264",
        "-preset",
        config.render_preset,
        "-crf",
        str(config.render_crf),
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        output_path,
    ]

    logger.debug(f"Running letterbox render: {' '.join(cmd)}")

    from app.core.utils.ffmpeg import run_ffmpeg
    
    try:
        run_ffmpeg(cmd, suppress_warnings=True, log_level="error", check=True)
    except RuntimeError as e:
        logger.error(f"FFmpeg letterbox render failed: {e}")
        raise
