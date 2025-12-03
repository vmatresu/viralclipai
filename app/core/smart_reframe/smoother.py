"""
Camera path smoothing for jitter-free virtual camera motion.

This module provides temporal smoothing of camera positions
to create smooth, professional-looking reframing.
"""

import logging
from typing import Optional

import numpy as np

from app.core.smart_reframe.models import (
    CameraKeyframe,
    CameraMode,
    ShotCameraPlan,
    Shot,
    ShotDetections,
    BoundingBox,
)
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.smart_reframe.saliency import SaliencyEstimator

logger = logging.getLogger(__name__)


class CameraSmoother:
    """
    Smooth camera paths to eliminate jitter and enforce motion constraints.
    """

    def __init__(self, config: IntelligentCropConfig, fps: float):
        self.config = config
        self.fps = fps
        self.saliency = SaliencyEstimator()

    def compute_camera_plan(
        self,
        shot: Shot,
        detections: ShotDetections,
        frame_width: int,
        frame_height: int,
        video_path: Optional[str] = None,
    ) -> ShotCameraPlan:
        """
        Compute a smooth camera plan for a shot.

        Args:
            shot: The shot to process.
            detections: Detections for this shot.
            frame_width: Source video width.
            frame_height: Source video height.
            video_path: Path to video for saliency fallback.

        Returns:
            ShotCameraPlan with smoothed keyframes.
        """
        # Generate raw focus points from detections
        raw_keyframes = self._compute_raw_focus(
            shot, detections, frame_width, frame_height, video_path
        )

        if not raw_keyframes:
            # Fallback to center if no focus points
            return ShotCameraPlan(
                shot_id=shot.id,
                mode=CameraMode.STATIC,
                keyframes=[
                    CameraKeyframe(
                        time=shot.start_time,
                        cx=frame_width / 2,
                        cy=frame_height / 2,
                        width=frame_width * 0.6,
                        height=frame_height * 0.6,
                    )
                ],
            )

        # Determine camera mode based on motion
        mode = self._classify_camera_mode(raw_keyframes)

        # Apply smoothing
        if mode == CameraMode.STATIC:
            smoothed = self._smooth_static(raw_keyframes)
        else:
            smoothed = self._smooth_tracking(raw_keyframes)

        # Enforce motion constraints
        constrained = self._enforce_constraints(smoothed, frame_width, frame_height)

        return ShotCameraPlan(
            shot_id=shot.id,
            mode=mode,
            keyframes=constrained,
        )

    def _compute_raw_focus(
        self,
        shot: Shot,
        detections: ShotDetections,
        frame_width: int,
        frame_height: int,
        video_path: Optional[str] = None,
    ) -> list[CameraKeyframe]:
        """
        Compute raw focus points from detections.
        """
        # Group detections by time
        time_to_dets: dict[float, list] = {}
        for det in detections.detections:
            if shot.start_time <= det.time <= shot.end_time:
                if det.time not in time_to_dets:
                    time_to_dets[det.time] = []
                time_to_dets[det.time].append(det)

        keyframes = []
        sample_interval = 1.0 / self.config.fps_sample

        # Generate keyframes at regular intervals
        t = shot.start_time
        while t <= shot.end_time:
            # Find closest detection time
            closest_time = None
            min_diff = float("inf")
            for det_time in time_to_dets:
                diff = abs(det_time - t)
                if diff < min_diff and diff < sample_interval:
                    min_diff = diff
                    closest_time = det_time

            if closest_time is not None:
                dets = time_to_dets[closest_time]
                focus = self._compute_focus_from_detections(
                    dets, frame_width, frame_height
                )
                keyframes.append(
                    CameraKeyframe(
                        time=t,
                        cx=focus.cx,
                        cy=focus.cy,
                        width=focus.width,
                        height=focus.height,
                    )
                )
            else:
                # No detection - use fallback
                focus = self.saliency.get_focus_region(
                    np.zeros((frame_height, frame_width, 3), dtype=np.uint8),
                    self.config.fallback_policy,
                )
                keyframes.append(
                    CameraKeyframe(
                        time=t,
                        cx=focus.cx,
                        cy=focus.cy,
                        width=focus.width,
                        height=focus.height,
                    )
                )

            t += sample_interval

        return keyframes

    def _compute_focus_from_detections(
        self,
        detections: list,
        frame_width: int,
        frame_height: int,
    ) -> BoundingBox:
        """
        Compute focus region from a set of detections.
        """
        if not detections:
            return BoundingBox(
                x=frame_width * 0.2,
                y=frame_height * 0.2,
                width=frame_width * 0.6,
                height=frame_height * 0.6,
            )

        if self.config.prefer_primary_subject and len(detections) > 1:
            # Use the largest/most confident detection
            primary = max(detections, key=lambda d: d.bbox.area * d.score)
            focus_box = primary.bbox.pad(
                primary.bbox.width * self.config.subject_padding
            )
        else:
            # Combine all detections
            boxes = [d.bbox for d in detections]
            combined = BoundingBox.union(boxes)
            if combined:
                focus_box = combined.pad(combined.width * self.config.subject_padding)
            else:
                focus_box = BoundingBox(
                    x=frame_width * 0.2,
                    y=frame_height * 0.2,
                    width=frame_width * 0.6,
                    height=frame_height * 0.6,
                )

        return focus_box.clamp(frame_width, frame_height)

    def _classify_camera_mode(self, keyframes: list[CameraKeyframe]) -> CameraMode:
        """
        Classify the camera mode based on motion analysis.
        """
        if len(keyframes) < 2:
            return CameraMode.STATIC

        # Compute motion statistics
        cx_values = [kf.cx for kf in keyframes]
        cy_values = [kf.cy for kf in keyframes]
        width_values = [kf.width for kf in keyframes]

        cx_range = max(cx_values) - min(cx_values)
        cy_range = max(cy_values) - min(cy_values)
        width_range = max(width_values) - min(width_values)

        # Compute standard deviations
        cx_std = np.std(cx_values)
        cy_std = np.std(cy_values)
        width_std = np.std(width_values)

        # Thresholds (relative to average values)
        avg_width = np.mean(width_values)
        motion_threshold = avg_width * 0.1  # 10% of focus width
        zoom_threshold = avg_width * 0.15  # 15% of focus width

        if width_std > zoom_threshold:
            return CameraMode.ZOOM
        elif cx_std > motion_threshold or cy_std > motion_threshold:
            return CameraMode.TRACKING
        else:
            return CameraMode.STATIC

    def _smooth_static(
        self, keyframes: list[CameraKeyframe]
    ) -> list[CameraKeyframe]:
        """
        Smooth keyframes for static camera mode (use average position).
        """
        if not keyframes:
            return []

        # Use median for robustness to outliers
        avg_cx = np.median([kf.cx for kf in keyframes])
        avg_cy = np.median([kf.cy for kf in keyframes])
        avg_width = np.median([kf.width for kf in keyframes])
        avg_height = np.median([kf.height for kf in keyframes])

        return [
            CameraKeyframe(
                time=kf.time,
                cx=avg_cx,
                cy=avg_cy,
                width=avg_width,
                height=avg_height,
            )
            for kf in keyframes
        ]

    def _smooth_tracking(
        self, keyframes: list[CameraKeyframe]
    ) -> list[CameraKeyframe]:
        """
        Smooth keyframes for tracking camera mode (apply low-pass filter).
        """
        if len(keyframes) < 3:
            return keyframes

        # Extract arrays
        times = np.array([kf.time for kf in keyframes])
        cx = np.array([kf.cx for kf in keyframes])
        cy = np.array([kf.cy for kf in keyframes])
        width = np.array([kf.width for kf in keyframes])
        height = np.array([kf.height for kf in keyframes])

        # Compute window size in samples
        sample_rate = len(times) / (times[-1] - times[0]) if times[-1] > times[0] else 1
        window_samples = max(3, int(self.config.smoothing_window * sample_rate))
        if window_samples % 2 == 0:
            window_samples += 1

        # Apply moving average
        cx_smooth = self._moving_average(cx, window_samples)
        cy_smooth = self._moving_average(cy, window_samples)
        width_smooth = self._moving_average(width, window_samples)
        height_smooth = self._moving_average(height, window_samples)

        return [
            CameraKeyframe(
                time=float(times[i]),
                cx=float(cx_smooth[i]),
                cy=float(cy_smooth[i]),
                width=float(width_smooth[i]),
                height=float(height_smooth[i]),
            )
            for i in range(len(times))
        ]

    def _moving_average(self, data: np.ndarray, window: int) -> np.ndarray:
        """Apply moving average with edge handling."""
        if len(data) < window:
            return data

        # Pad edges
        pad = window // 2
        padded = np.pad(data, (pad, pad), mode="edge")

        # Compute cumulative sum for efficient moving average
        cumsum = np.cumsum(padded)
        cumsum = np.insert(cumsum, 0, 0)

        result = (cumsum[window:] - cumsum[:-window]) / window
        return result

    def _enforce_constraints(
        self,
        keyframes: list[CameraKeyframe],
        frame_width: int,
        frame_height: int,
    ) -> list[CameraKeyframe]:
        """
        Enforce motion and boundary constraints on keyframes.
        """
        if len(keyframes) < 2:
            return keyframes

        constrained = [keyframes[0]]

        for i in range(1, len(keyframes)):
            prev = constrained[-1]
            curr = keyframes[i]

            dt = curr.time - prev.time
            if dt <= 0:
                constrained.append(curr)
                continue

            # Compute velocity
            dx = curr.cx - prev.cx
            dy = curr.cy - prev.cy
            speed = np.sqrt(dx**2 + dy**2) / dt

            # Limit speed
            if speed > self.config.max_pan_speed:
                scale = self.config.max_pan_speed / speed
                new_cx = prev.cx + dx * scale
                new_cy = prev.cy + dy * scale
            else:
                new_cx = curr.cx
                new_cy = curr.cy

            # Clamp to frame bounds (with margin for crop window)
            margin_x = curr.width / 2
            margin_y = curr.height / 2
            new_cx = np.clip(new_cx, margin_x, frame_width - margin_x)
            new_cy = np.clip(new_cy, margin_y, frame_height - margin_y)

            constrained.append(
                CameraKeyframe(
                    time=curr.time,
                    cx=float(new_cx),
                    cy=float(new_cy),
                    width=curr.width,
                    height=curr.height,
                )
            )

        return constrained


def compute_camera_plans(
    shots: list[Shot],
    all_detections: list[ShotDetections],
    frame_width: int,
    frame_height: int,
    fps: float,
    config: Optional[IntelligentCropConfig] = None,
) -> list[ShotCameraPlan]:
    """
    Compute camera plans for all shots.

    Args:
        shots: List of shots.
        all_detections: Detections for each shot.
        frame_width: Source video width.
        frame_height: Source video height.
        fps: Video frame rate.
        config: Configuration options.

    Returns:
        List of ShotCameraPlan objects.
    """
    if config is None:
        config = IntelligentCropConfig()

    smoother = CameraSmoother(config, fps)

    # Create a map of shot_id to detections
    det_map = {sd.shot_id: sd for sd in all_detections}

    plans = []
    for shot in shots:
        detections = det_map.get(shot.id, ShotDetections(shot_id=shot.id))
        plan = smoother.compute_camera_plan(
            shot, detections, frame_width, frame_height
        )
        plans.append(plan)

    return plans
