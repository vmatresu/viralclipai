"""
Camera path smoothing for jitter-free virtual camera motion.

This module provides temporal smoothing of camera positions
to create smooth, professional-looking reframing.
"""

import logging
from typing import Optional

import cv2
import numpy as np

from app.core.smart_reframe.models import (
    CameraKeyframe,
    CameraMode,
    ShotCameraPlan,
    Shot,
    ShotDetections,
    BoundingBox,
    Detection,
)
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.smart_reframe.saliency import SaliencyEstimator
from app.core.smart_reframe.face_activity import (
    FaceActivityAnalyzer,
    TemporalActivityTracker,
)
from app.core.utils.opencv import suppress_ffmpeg_warnings

logger = logging.getLogger(__name__)


class CameraSmoother:
    """
    Smooth camera paths to eliminate jitter and enforce motion constraints.
    """

    def __init__(self, config: IntelligentCropConfig, fps: float):
        self.config = config
        self.fps = fps
        self.saliency = SaliencyEstimator()
        self.activity_analyzer: Optional[FaceActivityAnalyzer] = None
        self.activity_tracker: Optional[TemporalActivityTracker] = None
        
        if config.enable_multi_face_activity:
            self.activity_analyzer = FaceActivityAnalyzer(config)
            self.activity_tracker = TemporalActivityTracker(config)

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
        # Check if we need activity analysis for multi-face scenarios
        # Only analyze if there are multiple distinct tracks
        unique_tracks = len(set(d.track_id for d in detections.detections))
        needs_activity_analysis = (
            self.config.enable_multi_face_activity
            and video_path is not None
            and self.activity_analyzer is not None
            and self.activity_tracker is not None
            and unique_tracks > 1  # Only needed for multiple faces
        )
        
        # Analyze activity if needed
        if needs_activity_analysis:
            self._analyze_face_activity(shot, detections, video_path, frame_width, frame_height)

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
                    dets, frame_width, frame_height, t
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
        detections: list[Detection],
        frame_width: int,
        frame_height: int,
        current_time: float,
    ) -> BoundingBox:
        """
        Compute focus region from a set of detections.
        
        Uses activity-based selection for multi-face scenarios.
        """
        if not detections:
            return BoundingBox(
                x=frame_width * 0.2,
                y=frame_height * 0.2,
                width=frame_width * 0.6,
                height=frame_height * 0.6,
            )

        # Check if this is a multi-face scenario requiring activity analysis
        if (
            len(detections) > 1
            and self.config.enable_multi_face_activity
            and self.activity_tracker is not None
        ):
            # Check if faces are far apart
            faces_far_apart = self._are_faces_far_apart(detections, frame_width)
            
            if faces_far_apart:
                # Use activity-based selection
                selected_track_id = self.activity_tracker.select_active_face(
                    [d.track_id for d in detections],
                    current_time
                )
                
                if selected_track_id is not None:
                    # Find detection with selected track ID
                    selected_det = next(
                        (d for d in detections if d.track_id == selected_track_id),
                        None
                    )
                    if selected_det:
                        focus_box = selected_det.bbox.pad(
                            selected_det.bbox.width * self.config.subject_padding
                        )
                        return focus_box.clamp(frame_width, frame_height)

        # Fallback to original logic
        if self.config.prefer_primary_subject and len(detections) > 1:
            # Use the largest/most confident detection
            primary = max(detections, key=lambda d: d.bbox.area * d.score)
            focus_box = primary.bbox.pad(
                primary.bbox.width * self.config.subject_padding
            )
        else:
            # Check if faces are close enough to combine
            faces_far_apart = self._are_faces_far_apart(detections, frame_width)
            if not faces_far_apart:
                # Combine all detections (they're close together)
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
            else:
                # Faces far apart but activity analysis not enabled/available
                # Use primary subject as fallback
                primary = max(detections, key=lambda d: d.bbox.area * d.score)
                focus_box = primary.bbox.pad(
                    primary.bbox.width * self.config.subject_padding
                )

        return focus_box.clamp(frame_width, frame_height)

    def _are_faces_far_apart(
        self, detections: list[Detection], frame_width: int
    ) -> bool:
        """
        Check if faces are far apart (should use activity-based selection).
        
        Args:
            detections: List of detections.
            frame_width: Frame width for normalization.
            
        Returns:
            True if faces are far apart.
        """
        if len(detections) < 2:
            return False
        
        # Compute distance between face centers
        centers = [(d.bbox.cx, d.bbox.cy) for d in detections]
        
        max_distance = 0.0
        for i in range(len(centers)):
            for j in range(i + 1, len(centers)):
                dx = centers[i][0] - centers[j][0]
                dy = centers[i][1] - centers[j][1]
                distance = np.sqrt(dx**2 + dy**2)
                max_distance = max(max_distance, distance)
        
        # Normalize by frame width
        normalized_distance = max_distance / frame_width
        
        return normalized_distance > self.config.multi_face_separation_threshold

    def _analyze_face_activity(
        self,
        shot: Shot,
        detections: ShotDetections,
        video_path: str,
        frame_width: int,
        frame_height: int,
    ):
        """
        Analyze face activity for multi-face scenarios.
        
        Reads video frames and computes activity scores for each face.
        """
        if self.activity_analyzer is None or self.activity_tracker is None:
            return
        
        try:
            with suppress_ffmpeg_warnings():
                cap = cv2.VideoCapture(video_path)
            if not cap.isOpened():
                logger.warning(f"Failed to open video for activity analysis: {video_path}")
                return

            try:
                fps = cap.get(cv2.CAP_PROP_FPS)
                start_frame = int(shot.start_time * fps)
                end_frame = int(shot.end_time * fps)
                
                # Sample at activity analysis rate (can be lower than detection rate)
                activity_sample_rate = max(1, int(fps / min(self.config.fps_sample * 2, 10)))
                
                # Group detections by frame
                detections_by_frame: dict[int, list[Detection]] = {}
                for det in detections.detections:
                    if shot.start_time <= det.time <= shot.end_time:
                        frame_num = int(det.time * fps)
                        if frame_num not in detections_by_frame:
                            detections_by_frame[frame_num] = []
                        detections_by_frame[frame_num].append(det)
                
                cap.set(cv2.CAP_PROP_POS_FRAMES, start_frame)
                frame_idx = start_frame
                
                while frame_idx < end_frame:
                    ret, frame = cap.read()
                    if not ret:
                        break
                    
                    timestamp = frame_idx / fps
                    
                    # Get detections at this frame (or nearby)
                    frame_dets = detections_by_frame.get(frame_idx, [])
                    
                    # If no exact match, find closest
                    if not frame_dets:
                        closest_frame = None
                        min_diff = float("inf")
                        for det_frame in detections_by_frame:
                            diff = abs(det_frame - frame_idx)
                            if diff < min_diff and diff < activity_sample_rate:
                                min_diff = diff
                                closest_frame = det_frame
                        if closest_frame is not None:
                            frame_dets = detections_by_frame[closest_frame]
                    
                    # Compute activity scores
                    for det in frame_dets:
                        activity_score = self.activity_analyzer.compute_activity_score(
                            frame, det
                        )
                        self.activity_tracker.update_activity(
                            det.track_id, activity_score, timestamp
                        )
                    
                    # Skip frames
                    frame_idx += activity_sample_rate
                    cap.set(cv2.CAP_PROP_POS_FRAMES, frame_idx)
                    
            finally:
                cap.release()
                
        except Exception as e:
            logger.warning(f"Error during face activity analysis: {e}")

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
    video_path: Optional[str] = None,
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
        video_path: Optional path to video file for activity analysis.

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
            shot, detections, frame_width, frame_height, video_path
        )
        plans.append(plan)

    # Cleanup activity analyzer resources
    if smoother.activity_analyzer is not None:
        smoother.activity_analyzer.close()

    return plans
