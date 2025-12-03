"""
Smart Reframe - Intelligent video cropping for ViralClipAI.

This module provides automatic reframing of horizontal/widescreen videos
into vertical/portrait formats (9:16, 4:5, 1:1) while keeping important
subjects well-framed.

Usage:
    from app.core.smart_reframe import Reframer, AspectRatio, IntelligentCropConfig

    reframer = Reframer(
        target_aspect_ratios=[AspectRatio(9, 16)],
        config=IntelligentCropConfig(fps_sample=3)
    )

    # Option 1: Analyze and render in one call
    output_paths = reframer.analyze_and_render(
        input_path="input.mp4",
        output_prefix="output_portrait"
    )

    # Option 2: Decouple analysis from rendering
    crop_plan = reframer.analyze(input_path="input.mp4")
    output_paths = reframer.render(input_path="input.mp4", crop_plan=crop_plan)
"""

from app.core.smart_reframe.models import (
    AspectRatio,
    BoundingBox,
    Shot,
    Detection,
    ShotDetections,
    CameraMode,
    CameraKeyframe,
    ShotCameraPlan,
    CropWindow,
    ShotCropPlan,
    VideoMeta,
    CropPlan,
)
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.smart_reframe.reframer import Reframer, analyze_and_render
from app.core.smart_reframe.cache import ShotDetectionCache, get_shot_cache, detect_shots_cached
from app.core.smart_reframe.config_factory import (
    get_production_config,
    get_fast_config,
    get_balanced_config,
    get_quality_config,
    get_config_from_env,
)

__all__ = [
    # Main class
    "Reframer",
    "analyze_and_render",
    # Configuration
    "IntelligentCropConfig",
    "get_production_config",
    "get_fast_config",
    "get_balanced_config",
    "get_quality_config",
    "get_config_from_env",
    # Caching
    "ShotDetectionCache",
    "get_shot_cache",
    "detect_shots_cached",
    # Data models
    "AspectRatio",
    "BoundingBox",
    "Shot",
    "Detection",
    "ShotDetections",
    "CameraMode",
    "CameraKeyframe",
    "ShotCameraPlan",
    "CropWindow",
    "ShotCropPlan",
    "VideoMeta",
    "CropPlan",
]

__version__ = "1.0.0"
