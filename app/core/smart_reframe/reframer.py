"""
Main Reframer class that orchestrates the intelligent cropping pipeline.

This is the primary entry point for the smart reframe functionality.
"""

import logging
from pathlib import Path
from typing import Optional

import cv2

from app.core.smart_reframe.models import (
    AspectRatio,
    CropPlan,
    Shot,
    VideoMeta,
)
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.smart_reframe.shot_detector import ShotDetector
from app.core.smart_reframe.content_analyzer import ContentAnalyzer
from app.core.smart_reframe.smoother import compute_camera_plans
from app.core.smart_reframe.crop_planner import compute_crop_plans
from app.core.smart_reframe.renderer import Renderer
from app.core.smart_reframe.cache import detect_shots_cached, ShotDetectionCache

logger = logging.getLogger(__name__)


class Reframer:
    """
    Intelligent video reframer.

    Analyzes videos to detect faces/persons and generates smooth
    crop plans for portrait/square aspect ratios.

    Example usage:
        reframer = Reframer(
            target_aspect_ratios=[AspectRatio(9, 16)],
            config=IntelligentCropConfig(fps_sample=3)
        )

        # Option 1: All-in-one
        output_paths = reframer.analyze_and_render(
            input_path="input.mp4",
            output_prefix="output"
        )

        # Option 2: Separate analysis and rendering
        crop_plan = reframer.analyze(input_path="input.mp4")
        crop_plan.to_json_file("cropplan.json")
        output_paths = reframer.render(input_path="input.mp4", crop_plan=crop_plan)
    """

    def __init__(
        self,
        target_aspect_ratios: Optional[list[AspectRatio]] = None,
        config: Optional[IntelligentCropConfig] = None,
        shot_cache: Optional[ShotDetectionCache] = None,
    ):
        """
        Initialize the reframer.

        Args:
            target_aspect_ratios: List of target aspect ratios.
                                  Defaults to [9:16] if not provided.
            config: Configuration options. Uses defaults if not provided.
            shot_cache: Optional shot detection cache for performance.
        """
        if target_aspect_ratios is None:
            target_aspect_ratios = [AspectRatio(width=9, height=16)]
        if config is None:
            config = IntelligentCropConfig()

        self.target_aspect_ratios = target_aspect_ratios
        self.config = config
        self.shot_cache = shot_cache

        # Initialize pipeline components
        self._shot_detector = ShotDetector(config)
        self._content_analyzer = ContentAnalyzer(config)
        self._renderer = Renderer(config)

    def analyze(
        self,
        input_path: str,
        time_range: Optional[tuple[float, float]] = None,
    ) -> CropPlan:
        """
        Analyze a video and generate a crop plan.

        Args:
            input_path: Path to the source video.
            time_range: Optional (start, end) time range to analyze.

        Returns:
            CropPlan containing all analysis results and crop windows.
        """
        logger.info(f"Analyzing video: {input_path}")

        # Get video metadata
        video_meta = self._get_video_meta(input_path)
        logger.info(
            f"Video: {video_meta.width}x{video_meta.height} @ {video_meta.fps:.2f}fps, "
            f"duration: {video_meta.duration:.2f}s"
        )

        # Apply time range
        if time_range:
            effective_duration = time_range[1] - time_range[0]
            logger.info(
                f"Analyzing time range: {time_range[0]:.2f}s - {time_range[1]:.2f}s"
            )
        else:
            effective_duration = video_meta.duration

        # Step 1: Shot detection (with caching)
        logger.info("Step 1/4: Detecting shots...")
        if self.shot_cache is not None and time_range is None:
            shots = detect_shots_cached(input_path, self.config, time_range, self.shot_cache)
        else:
            shots = self._shot_detector.detect_shots(input_path, time_range)
        logger.info(f"  Found {len(shots)} shots")

        # If no shots detected, create one covering the whole range
        if not shots:
            if time_range:
                shots = [
                    Shot(id=0, start_time=time_range[0], end_time=time_range[1])
                ]
            else:
                shots = [Shot(id=0, start_time=0, end_time=video_meta.duration)]

        # Step 2: Content analysis
        logger.info("Step 2/4: Analyzing content (face detection)...")
        shot_detections = self._content_analyzer.analyze_video(input_path, shots)

        total_detections = sum(len(sd.detections) for sd in shot_detections)
        logger.info(f"  Found {total_detections} face detections across all shots")

        # Step 3: Camera path planning
        logger.info("Step 3/4: Computing camera paths...")
        camera_plans = compute_camera_plans(
            shots,
            shot_detections,
            video_meta.width,
            video_meta.height,
            video_meta.fps,
            self.config,
        )
        logger.info(f"  Generated {len(camera_plans)} camera plans")

        # Step 4: Crop window computation
        logger.info("Step 4/4: Computing crop windows...")
        crop_plans = compute_crop_plans(
            shots,
            camera_plans,
            self.target_aspect_ratios,
            video_meta.width,
            video_meta.height,
            self.config,
        )
        logger.info(
            f"  Generated {len(crop_plans)} crop plans "
            f"for {len(self.target_aspect_ratios)} aspect ratio(s)"
        )

        # Build complete crop plan
        crop_plan = CropPlan(
            video=video_meta,
            target_aspect_ratios=self.target_aspect_ratios,
            shots=shots,
            shot_detections=shot_detections,
            shot_camera_plans=camera_plans,
            shot_crop_plans=crop_plans,
        )

        logger.info("Analysis complete")
        return crop_plan

    def render(
        self,
        input_path: str,
        crop_plan: CropPlan,
        output_prefix: Optional[str] = None,
        output_resolution: Optional[tuple[int, int]] = None,
    ) -> dict[str, str]:
        """
        Render reframed videos from a crop plan.

        Args:
            input_path: Path to the source video.
            crop_plan: Previously computed crop plan.
            output_prefix: Prefix for output files. Defaults to input name.
            output_resolution: Optional (width, height) for output videos.

        Returns:
            Dict mapping aspect ratio string to output file path.
        """
        if output_prefix is None:
            input_stem = Path(input_path).stem
            output_prefix = str(Path(input_path).parent / f"{input_stem}_reframed")

        logger.info(f"Rendering reframed videos to: {output_prefix}_*.mp4")

        output_paths = self._renderer.render(
            input_path,
            crop_plan,
            output_prefix,
            output_resolution,
        )

        logger.info(f"Rendered {len(output_paths)} video(s)")
        return output_paths

    def analyze_and_render(
        self,
        input_path: str,
        output_prefix: Optional[str] = None,
        time_range: Optional[tuple[float, float]] = None,
        output_resolution: Optional[tuple[int, int]] = None,
        save_crop_plan: bool = False,
    ) -> dict[str, str]:
        """
        Analyze and render in one call.

        Args:
            input_path: Path to the source video.
            output_prefix: Prefix for output files. Defaults to input name.
            time_range: Optional (start, end) time range to process.
            output_resolution: Optional (width, height) for output videos.
            save_crop_plan: If True, save the crop plan as JSON.

        Returns:
            Dict mapping aspect ratio string to output file path.
        """
        # Analyze
        crop_plan = self.analyze(input_path, time_range)

        # Optionally save crop plan
        if save_crop_plan:
            if output_prefix is None:
                plan_path = f"{Path(input_path).stem}.cropplan.json"
            else:
                plan_path = f"{output_prefix}.cropplan.json"
            crop_plan.to_json_file(plan_path)
            logger.info(f"Saved crop plan to: {plan_path}")

        # Render
        return self.render(input_path, crop_plan, output_prefix, output_resolution)

    def _get_video_meta(self, video_path: str) -> VideoMeta:
        """Extract video metadata using OpenCV."""
        cap = cv2.VideoCapture(video_path)
        if not cap.isOpened():
            raise RuntimeError(f"Failed to open video: {video_path}")

        try:
            width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
            height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
            fps = cap.get(cv2.CAP_PROP_FPS)
            frame_count = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
            duration = frame_count / fps if fps > 0 else 0

            return VideoMeta(
                input_path=video_path,
                duration=duration,
                width=width,
                height=height,
                fps=fps,
            )
        finally:
            cap.release()

    def close(self):
        """Release resources."""
        self._content_analyzer.close()


def analyze_and_render(
    input_path: str,
    output_path: str,
    target_aspect_ratios: Optional[list[AspectRatio]] = None,
    crop_mode: str = "intelligent",
    time_range: Optional[tuple[float, float]] = None,
    config: Optional[IntelligentCropConfig] = None,
) -> dict[str, str]:
    """
    Convenience function for one-shot reframing.

    This is the main entry point for integration with ViralClipAI.

    Args:
        input_path: Path to source video.
        output_path: Path or prefix for output video(s).
        target_aspect_ratios: Target aspect ratios.
        crop_mode: Must be "intelligent" for this function.
        time_range: Optional (start, end) time range.
        config: Configuration options.

    Returns:
        Dict mapping aspect ratio string to output path.

    Raises:
        ValueError: If crop_mode is not "intelligent".
    """
    if crop_mode != "intelligent":
        raise ValueError(
            f"This function only supports crop_mode='intelligent', got '{crop_mode}'"
        )

    if target_aspect_ratios is None:
        target_aspect_ratios = [AspectRatio(width=9, height=16)]

    reframer = Reframer(
        target_aspect_ratios=target_aspect_ratios,
        config=config,
    )

    try:
        # Remove .mp4 extension if present for prefix
        output_prefix = output_path
        if output_prefix.endswith(".mp4"):
            output_prefix = output_prefix[:-4]

        return reframer.analyze_and_render(
            input_path=input_path,
            output_prefix=output_prefix,
            time_range=time_range,
            save_crop_plan=True,
        )
    finally:
        reframer.close()
