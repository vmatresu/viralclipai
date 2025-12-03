"""
Crop window computation for different aspect ratios.

This module converts camera plans to actual crop windows
while maintaining proper composition and aspect ratios.
"""

import logging
from typing import Optional

import numpy as np

from app.core.smart_reframe.models import (
    AspectRatio,
    CropWindow,
    Shot,
    ShotCameraPlan,
    ShotCropPlan,
    CameraKeyframe,
)
from app.core.smart_reframe.config import IntelligentCropConfig

logger = logging.getLogger(__name__)


class CropPlanner:
    """
    Compute crop windows for target aspect ratios.
    """

    def __init__(
        self,
        config: IntelligentCropConfig,
        frame_width: int,
        frame_height: int,
    ):
        self.config = config
        self.frame_width = frame_width
        self.frame_height = frame_height

    def compute_crop_plan(
        self,
        shot: Shot,
        camera_plan: ShotCameraPlan,
        aspect_ratio: AspectRatio,
    ) -> ShotCropPlan:
        """
        Compute crop windows for a shot and target aspect ratio.

        Args:
            shot: The shot to process.
            camera_plan: Camera plan for this shot.
            aspect_ratio: Target aspect ratio.

        Returns:
            ShotCropPlan with crop windows.
        """
        crop_windows = []

        for keyframe in camera_plan.keyframes:
            crop_window = self._keyframe_to_crop(keyframe, aspect_ratio)
            crop_windows.append(crop_window)

        return ShotCropPlan(
            shot_id=shot.id,
            aspect_ratio=aspect_ratio,
            crop_windows=crop_windows,
        )

    def _keyframe_to_crop(
        self,
        keyframe: CameraKeyframe,
        aspect_ratio: AspectRatio,
    ) -> CropWindow:
        """
        Convert a camera keyframe to a crop window.

        Maintains the target aspect ratio while keeping the
        subject centered with proper composition.
        """
        target_ratio = aspect_ratio.ratio
        source_ratio = self.frame_width / self.frame_height

        # Determine crop dimensions based on aspect ratios
        if target_ratio <= source_ratio:
            # Target is narrower than source (e.g., 9:16 from 16:9)
            # Height is constrained, width is cropped
            crop_height, crop_width = self._compute_narrow_crop(
                keyframe, target_ratio
            )
        else:
            # Target is wider than source (less common)
            # Width is constrained, may need letterboxing
            crop_width, crop_height = self._compute_wide_crop(
                keyframe, target_ratio
            )

        # Compute crop position centered on focus point
        x = keyframe.cx - crop_width / 2
        y = keyframe.cy - crop_height / 2

        # Apply headroom adjustment for faces
        # Shift up slightly to give more headroom
        headroom_shift = crop_height * self.config.headroom_ratio * 0.3
        y -= headroom_shift

        # Clamp to frame boundaries
        x = max(0, min(x, self.frame_width - crop_width))
        y = max(0, min(y, self.frame_height - crop_height))

        # Ensure integer values
        x = int(round(x))
        y = int(round(y))
        crop_width = int(round(crop_width))
        crop_height = int(round(crop_height))

        # Final bounds check
        if x + crop_width > self.frame_width:
            x = self.frame_width - crop_width
        if y + crop_height > self.frame_height:
            y = self.frame_height - crop_height

        return CropWindow(
            time=keyframe.time,
            x=max(0, x),
            y=max(0, y),
            width=max(1, crop_width),
            height=max(1, crop_height),
        )

    def _compute_narrow_crop(
        self,
        keyframe: CameraKeyframe,
        target_ratio: float,
    ) -> tuple[int, int]:
        """
        Compute crop dimensions for narrow target (e.g., 9:16).

        Returns (height, width).
        """
        # Start with focus region dimensions
        focus_width = keyframe.width
        focus_height = keyframe.height

        # Compute required crop to fit subject
        # The crop must contain the focus region with margins
        min_margin = self.config.safe_margin

        # Compute the minimum crop that contains the focus region
        required_height = focus_height * (1 + 2 * min_margin)
        required_width = required_height * target_ratio

        # Check if this fits in the source
        if required_width > self.frame_width:
            # Width limited - use full width
            crop_width = self.frame_width
            crop_height = int(crop_width / target_ratio)
        elif required_height > self.frame_height:
            # Height limited - use full height
            crop_height = self.frame_height
            crop_width = int(crop_height * target_ratio)
        else:
            # Both fit - use the computed dimensions
            crop_height = int(required_height)
            crop_width = int(required_width)

        # Apply zoom limits
        zoom_factor = self.frame_width / crop_width
        if zoom_factor > self.config.max_zoom_factor:
            # Too zoomed in - widen the crop
            crop_width = int(self.frame_width / self.config.max_zoom_factor)
            crop_height = int(crop_width / target_ratio)

        # Ensure crop fits in frame
        if crop_height > self.frame_height:
            crop_height = self.frame_height
            crop_width = int(crop_height * target_ratio)

        return crop_height, crop_width

    def _compute_wide_crop(
        self,
        keyframe: CameraKeyframe,
        target_ratio: float,
    ) -> tuple[int, int]:
        """
        Compute crop dimensions for wide target.

        Returns (width, height).
        """
        # Similar logic but constrained by width
        focus_width = keyframe.width
        min_margin = self.config.safe_margin

        required_width = focus_width * (1 + 2 * min_margin)
        required_height = required_width / target_ratio

        if required_width > self.frame_width:
            crop_width = self.frame_width
            crop_height = int(crop_width / target_ratio)
        elif required_height > self.frame_height:
            crop_height = self.frame_height
            crop_width = int(crop_height * target_ratio)
        else:
            crop_width = int(required_width)
            crop_height = int(required_height)

        # Ensure crop fits in frame
        if crop_width > self.frame_width:
            crop_width = self.frame_width
            crop_height = int(crop_width / target_ratio)

        return crop_width, crop_height


def compute_crop_plans(
    shots: list[Shot],
    camera_plans: list[ShotCameraPlan],
    target_aspect_ratios: list[AspectRatio],
    frame_width: int,
    frame_height: int,
    config: Optional[IntelligentCropConfig] = None,
) -> list[ShotCropPlan]:
    """
    Compute crop plans for all shots and aspect ratios.

    Args:
        shots: List of shots.
        camera_plans: Camera plans for each shot.
        target_aspect_ratios: Target aspect ratios to compute.
        frame_width: Source video width.
        frame_height: Source video height.
        config: Configuration options.

    Returns:
        List of ShotCropPlan objects (one per shot × aspect ratio).
    """
    if config is None:
        config = IntelligentCropConfig()

    planner = CropPlanner(config, frame_width, frame_height)

    # Create a map of shot_id to camera plan
    plan_map = {cp.shot_id: cp for cp in camera_plans}

    crop_plans = []
    for shot in shots:
        camera_plan = plan_map.get(shot.id)
        if camera_plan is None:
            continue

        for aspect_ratio in target_aspect_ratios:
            crop_plan = planner.compute_crop_plan(shot, camera_plan, aspect_ratio)
            crop_plans.append(crop_plan)

    return crop_plans


def interpolate_crop_window(
    crop_windows: list[CropWindow],
    time: float,
) -> Optional[CropWindow]:
    """
    Interpolate crop window at a specific time.

    Args:
        crop_windows: List of crop windows (sorted by time).
        time: Target time.

    Returns:
        Interpolated CropWindow or None if out of range.
    """
    if not crop_windows:
        return None

    # Handle edge cases
    if time <= crop_windows[0].time:
        return crop_windows[0]
    if time >= crop_windows[-1].time:
        return crop_windows[-1]

    # Find surrounding keyframes
    for i in range(len(crop_windows) - 1):
        if crop_windows[i].time <= time <= crop_windows[i + 1].time:
            prev = crop_windows[i]
            next_ = crop_windows[i + 1]

            # Linear interpolation
            t = (time - prev.time) / (next_.time - prev.time)

            return CropWindow(
                time=time,
                x=int(round(prev.x + t * (next_.x - prev.x))),
                y=int(round(prev.y + t * (next_.y - prev.y))),
                width=int(round(prev.width + t * (next_.width - prev.width))),
                height=int(round(prev.height + t * (next_.height - prev.height))),
            )

    return None
