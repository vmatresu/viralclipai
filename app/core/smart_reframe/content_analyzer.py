"""
Content analysis using MediaPipe for face/person detection.

This module provides face and person detection with simple IoU-based
tracking across frames.
"""

import logging
from typing import Optional

import cv2
import numpy as np

from app.core.smart_reframe.models import (
    BoundingBox,
    Detection,
    Shot,
    ShotDetections,
)
from app.core.smart_reframe.config import IntelligentCropConfig, DetectorBackend

logger = logging.getLogger(__name__)

# Lazy imports for optional dependencies
_mediapipe_face_detection = None
_mediapipe_pose = None


def _get_mediapipe_face_detection():
    """Lazy load MediaPipe face detection."""
    global _mediapipe_face_detection
    if _mediapipe_face_detection is None:
        import mediapipe as mp

        _mediapipe_face_detection = mp.solutions.face_detection
    return _mediapipe_face_detection


def _get_mediapipe_pose():
    """Lazy load MediaPipe pose detection."""
    global _mediapipe_pose
    if _mediapipe_pose is None:
        import mediapipe as mp

        _mediapipe_pose = mp.solutions.pose
    return _mediapipe_pose


class SimpleTracker:
    """
    Simple IoU-based tracker for maintaining identity across frames.

    Uses Hungarian algorithm approximation for matching detections
    between consecutive frames.
    """

    def __init__(self, iou_threshold: float = 0.3, max_gap: int = 10):
        self.iou_threshold = iou_threshold
        self.max_gap = max_gap
        self.tracks: dict[int, dict] = {}  # track_id -> {bbox, age, active}
        self.next_track_id = 0

    def update(
        self, detections: list[tuple[BoundingBox, float]]
    ) -> list[tuple[int, BoundingBox, float]]:
        """
        Update tracks with new detections.

        Args:
            detections: List of (bbox, score) tuples.

        Returns:
            List of (track_id, bbox, score) tuples.
        """
        if not detections:
            # Age all tracks
            for track_id in list(self.tracks.keys()):
                self.tracks[track_id]["age"] += 1
                if self.tracks[track_id]["age"] > self.max_gap:
                    del self.tracks[track_id]
            return []

        # Match detections to existing tracks using IoU
        matched = []
        unmatched_dets = list(range(len(detections)))
        unmatched_tracks = list(self.tracks.keys())

        # Greedy matching by IoU
        matches = []
        for det_idx, (bbox, score) in enumerate(detections):
            best_iou = self.iou_threshold
            best_track = None
            for track_id in unmatched_tracks:
                iou = bbox.iou(self.tracks[track_id]["bbox"])
                if iou > best_iou:
                    best_iou = iou
                    best_track = track_id

            if best_track is not None:
                matches.append((det_idx, best_track))
                unmatched_dets.remove(det_idx)
                unmatched_tracks.remove(best_track)

        # Update matched tracks
        for det_idx, track_id in matches:
            bbox, score = detections[det_idx]
            self.tracks[track_id] = {"bbox": bbox, "age": 0, "active": True}
            matched.append((track_id, bbox, score))

        # Create new tracks for unmatched detections
        for det_idx in unmatched_dets:
            bbox, score = detections[det_idx]
            self.tracks[self.next_track_id] = {"bbox": bbox, "age": 0, "active": True}
            matched.append((self.next_track_id, bbox, score))
            self.next_track_id += 1

        # Age unmatched tracks
        for track_id in unmatched_tracks:
            self.tracks[track_id]["age"] += 1
            self.tracks[track_id]["active"] = False
            if self.tracks[track_id]["age"] > self.max_gap:
                del self.tracks[track_id]

        return matched


class ContentAnalyzer:
    """
    Analyze video content for faces and persons using MediaPipe.
    """

    def __init__(self, config: IntelligentCropConfig):
        self.config = config
        self._face_detector = None
        self._pose_detector = None

    def _init_detectors(self):
        """Initialize detection models lazily."""
        if self.config.detector_backend == DetectorBackend.MEDIAPIPE:
            if self._face_detector is None:
                mp_face = _get_mediapipe_face_detection()
                self._face_detector = mp_face.FaceDetection(
                    model_selection=0,  # 0 for short-range, 1 for full-range
                    min_detection_confidence=self.config.min_detection_confidence,
                )
        elif self.config.detector_backend == DetectorBackend.YOLO:
            try:
                from ultralytics import YOLO

                if self._face_detector is None:
                    # Use YOLOv8n for person detection
                    self._face_detector = YOLO("yolov8n.pt")
            except ImportError:
                logger.warning("YOLO not available, falling back to MediaPipe")
                mp_face = _get_mediapipe_face_detection()
                self._face_detector = mp_face.FaceDetection(
                    model_selection=0,
                    min_detection_confidence=self.config.min_detection_confidence,
                )

    def analyze_shot(
        self,
        video_path: str,
        shot: Shot,
    ) -> ShotDetections:
        """
        Analyze a single shot for faces and persons.

        Args:
            video_path: Path to the video file.
            shot: Shot to analyze.

        Returns:
            ShotDetections containing all detections in the shot.
        """
        self._init_detectors()

        cap = cv2.VideoCapture(video_path)
        if not cap.isOpened():
            raise RuntimeError(f"Failed to open video: {video_path}")

        try:
            fps = cap.get(cv2.CAP_PROP_FPS)
            frame_width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
            frame_height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))

            # Calculate sampling interval
            sample_interval = max(1, int(fps / self.config.fps_sample))

            # Initialize tracker
            tracker = SimpleTracker(
                iou_threshold=self.config.iou_threshold,
                max_gap=self.config.max_track_gap,
            )

            detections = []
            start_frame = int(shot.start_time * fps)
            end_frame = int(shot.end_time * fps)
            
            # Early exit optimization: sample first few frames to check for faces
            # If no faces found in first samples, skip detailed analysis
            early_exit_samples = 3
            early_exit_frame_count = 0
            early_exit_threshold = 0.1  # Check first 10% of shot
            early_exit_frame_limit = start_frame + int((end_frame - start_frame) * early_exit_threshold)

            cap.set(cv2.CAP_PROP_POS_FRAMES, start_frame)
            frame_idx = start_frame

            while frame_idx < end_frame:
                ret, frame = cap.read()
                if not ret:
                    break

                timestamp = frame_idx / fps

                # Resize for analysis
                scale = self.config.analysis_resolution / frame_height
                if scale < 1:
                    small = cv2.resize(
                        frame,
                        (
                            int(frame_width * scale),
                            self.config.analysis_resolution,
                        ),
                    )
                else:
                    small = frame
                    scale = 1.0

                # Detect faces
                raw_dets = self._detect_faces(small, scale, frame_width, frame_height)

                # Update tracker
                tracked = tracker.update(raw_dets)

                # Early exit optimization: check first few frames
                if frame_idx < early_exit_frame_limit:
                    if len(raw_dets) == 0:
                        early_exit_frame_count += 1
                        if early_exit_frame_count >= early_exit_samples:
                            # No faces found in early samples, skip detailed analysis
                            logger.debug(f"  Early exit: No faces detected in first {early_exit_samples} samples")
                            break
                    else:
                        # Faces found, reset counter and continue with full analysis
                        early_exit_frame_count = 0

                # Create Detection objects
                for track_id, bbox, score in tracked:
                    # Filter by minimum face size
                    face_area_ratio = bbox.area / (frame_width * frame_height)
                    if face_area_ratio < self.config.min_face_size:
                        continue

                    detections.append(
                        Detection(
                            time=timestamp,
                            bbox=bbox,
                            score=score,
                            track_id=track_id,
                            type="face",
                        )
                    )

                # Skip frames
                frame_idx += sample_interval
                cap.set(cv2.CAP_PROP_POS_FRAMES, frame_idx)

            return ShotDetections(shot_id=shot.id, detections=detections)

        finally:
            cap.release()

    def _detect_faces(
        self,
        frame: np.ndarray,
        scale: float,
        orig_width: int,
        orig_height: int,
    ) -> list[tuple[BoundingBox, float]]:
        """
        Detect faces in a frame.

        Args:
            frame: BGR frame (possibly downscaled).
            scale: Scale factor applied to frame.
            orig_width: Original frame width.
            orig_height: Original frame height.

        Returns:
            List of (bbox, score) tuples in original coordinates.
        """
        if self.config.detector_backend == DetectorBackend.YOLO:
            return self._detect_faces_yolo(frame, scale, orig_width, orig_height)
        else:
            return self._detect_faces_mediapipe(frame, scale, orig_width, orig_height)

    def _detect_faces_mediapipe(
        self,
        frame: np.ndarray,
        scale: float,
        orig_width: int,
        orig_height: int,
    ) -> list[tuple[BoundingBox, float]]:
        """Detect faces using MediaPipe."""
        # Convert BGR to RGB
        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)

        results = self._face_detector.process(rgb)

        detections = []
        if results.detections:
            h, w = frame.shape[:2]
            for detection in results.detections:
                bbox = detection.location_data.relative_bounding_box

                # Convert relative to absolute coordinates in original frame
                x = bbox.xmin * orig_width
                y = bbox.ymin * orig_height
                width = bbox.width * orig_width
                height = bbox.height * orig_height

                # Expand box slightly to include more of the head
                expand = 0.3
                x -= width * expand / 2
                y -= height * expand
                width *= 1 + expand
                height *= 1 + expand * 1.5

                box = BoundingBox(x=x, y=y, width=width, height=height)
                box = box.clamp(orig_width, orig_height)

                detections.append((box, detection.score[0]))

        return detections

    def _detect_faces_yolo(
        self,
        frame: np.ndarray,
        scale: float,
        orig_width: int,
        orig_height: int,
    ) -> list[tuple[BoundingBox, float]]:
        """Detect persons using YOLO (optional backend)."""
        results = self._face_detector(frame, verbose=False, classes=[0])  # person class

        detections = []
        for r in results:
            boxes = r.boxes
            for i in range(len(boxes)):
                xyxy = boxes.xyxy[i].cpu().numpy()
                conf = boxes.conf[i].cpu().numpy()

                # Scale back to original coordinates
                x1, y1, x2, y2 = xyxy / scale

                box = BoundingBox(x=x1, y=y1, width=x2 - x1, height=y2 - y1)
                box = box.clamp(orig_width, orig_height)

                detections.append((box, float(conf)))

        return detections

    def analyze_video(
        self,
        video_path: str,
        shots: list[Shot],
    ) -> list[ShotDetections]:
        """
        Analyze all shots in a video.

        Args:
            video_path: Path to the video file.
            shots: List of shots to analyze.

        Returns:
            List of ShotDetections, one per shot.
        """
        all_detections = []
        for shot in shots:
            logger.debug(f"Analyzing shot {shot.id}: {shot.start_time:.2f}s - {shot.end_time:.2f}s")
            shot_dets = self.analyze_shot(video_path, shot)
            all_detections.append(shot_dets)
            logger.debug(f"  Found {len(shot_dets.detections)} detections")

        return all_detections

    def close(self):
        """Release detector resources."""
        if self._face_detector is not None:
            if hasattr(self._face_detector, "close"):
                self._face_detector.close()
            self._face_detector = None
        if self._pose_detector is not None:
            if hasattr(self._pose_detector, "close"):
                self._pose_detector.close()
            self._pose_detector = None


def analyze_content(
    video_path: str,
    shots: list[Shot],
    config: Optional[IntelligentCropConfig] = None,
) -> list[ShotDetections]:
    """
    Convenience function for content analysis.

    Args:
        video_path: Path to the video file.
        shots: List of shots to analyze.
        config: Configuration options. Uses defaults if not provided.

    Returns:
        List of ShotDetections.
    """
    if config is None:
        config = IntelligentCropConfig()

    analyzer = ContentAnalyzer(config)
    try:
        return analyzer.analyze_video(video_path, shots)
    finally:
        analyzer.close()
