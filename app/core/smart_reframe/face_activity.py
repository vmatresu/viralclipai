"""
Face activity analysis for multi-face scenarios.

This module detects which face is active/speaking using visual cues:
- Mouth movement detection (MediaPipe Face Mesh)
- Motion/optical flow around faces
- Face size and confidence changes
"""

import logging
from collections import defaultdict
from typing import Optional

import cv2
import numpy as np

from app.core.smart_reframe.models import Detection, BoundingBox
from app.core.smart_reframe.config import IntelligentCropConfig

logger = logging.getLogger(__name__)

# Lazy import for MediaPipe Face Mesh
_mediapipe_face_mesh = None


def _get_mediapipe_face_mesh():
    """Lazy load MediaPipe Face Mesh."""
    global _mediapipe_face_mesh
    if _mediapipe_face_mesh is None:
        import mediapipe as mp

        _mediapipe_face_mesh = mp.solutions.face_mesh
    return _mediapipe_face_mesh


class FaceActivityAnalyzer:
    """
    Analyze face activity to determine which face is speaking/active.
    
    Uses visual cues: mouth movement, motion, and size changes.
    """

    def __init__(self, config: IntelligentCropConfig):
        self.config = config
        self._face_mesh = None
        self._prev_frames: dict[int, np.ndarray] = {}  # track_id -> previous frame region
        self._face_history: dict[int, list] = defaultdict(list)  # track_id -> history of (size, confidence, time)
        
        if config.enable_mouth_detection:
            self._init_face_mesh()

    def _init_face_mesh(self):
        """Initialize MediaPipe Face Mesh for mouth detection."""
        try:
            mp_face_mesh = _get_mediapipe_face_mesh()
            self._face_mesh = mp_face_mesh.FaceMesh(
                static_image_mode=False,
                max_num_faces=10,
                refine_landmarks=True,
                min_detection_confidence=0.5,
                min_tracking_confidence=0.5,
            )
        except Exception as e:
            logger.warning(f"Failed to initialize Face Mesh: {e}. Mouth detection disabled.")
            self._face_mesh = None

    def compute_mouth_openness(
        self, frame: np.ndarray, bbox: BoundingBox
    ) -> Optional[float]:
        """
        Compute mouth openness score for a face region.
        
        Args:
            frame: Full BGR frame.
            bbox: Face bounding box.
            
        Returns:
            Mouth openness score (0.0 = closed, 1.0 = fully open) or None if unavailable.
        """
        if self._face_mesh is None:
            return None

        try:
            # Extract face region with padding
            h, w = frame.shape[:2]
            pad = 0.2
            x1 = max(0, int(bbox.x - bbox.width * pad))
            y1 = max(0, int(bbox.y - bbox.height * pad))
            x2 = min(w, int(bbox.x2 + bbox.width * pad))
            y2 = min(h, int(bbox.y2 + bbox.height * pad))
            
            face_region = frame[y1:y2, x1:x2]
            if face_region.size == 0:
                return None

            # Convert to RGB for MediaPipe
            rgb = cv2.cvtColor(face_region, cv2.COLOR_BGR2RGB)
            
            # Process with Face Mesh
            results = self._face_mesh.process(rgb)
            
            if not results.multi_face_landmarks:
                return None

            # Use first face found (should be only one in cropped region)
            landmarks = results.multi_face_landmarks[0]
            
            # Mouth landmarks (MediaPipe Face Mesh indices)
            # Upper lip: 12, 13, 14, 15, 16, 17
            # Lower lip: 18, 19, 20, 21, 22, 23
            # Inner mouth: 61, 62, 63, 64, 65, 66, 67, 68
            upper_lip_indices = [12, 13, 14, 15, 16, 17]
            lower_lip_indices = [18, 19, 20, 21, 22, 23]
            
            # Get landmark coordinates
            # MediaPipe landmarks are relative to the full image, but we passed the face region
            # So landmarks are relative to the face region
            face_h, face_w = face_region.shape[:2]
            
            def get_landmark_y(indices):
                """Get average Y coordinate of landmarks."""
                ys = []
                for idx in indices:
                    landmark = landmarks.landmark[idx]
                    # Landmarks are normalized (0-1) relative to the image passed to process()
                    # Since we passed face_region, they're relative to face_region
                    y = landmark.y * face_h
                    ys.append(y)
                return np.mean(ys)
            
            upper_lip_y = get_landmark_y(upper_lip_indices)
            lower_lip_y = get_landmark_y(lower_lip_indices)
            
            # Mouth opening distance (normalized by face height)
            mouth_openness = abs(lower_lip_y - upper_lip_y) / face_h
            
            # Normalize to 0-1 range (typical values are 0.02-0.08)
            normalized = np.clip((mouth_openness - 0.02) / 0.06, 0.0, 1.0)
            
            return float(normalized)
            
        except Exception as e:
            logger.debug(f"Error computing mouth openness: {e}")
            return None

    def compute_motion_score(
        self,
        frame: np.ndarray,
        bbox: BoundingBox,
        track_id: int,
    ) -> float:
        """
        Compute motion/optical flow score around a face.
        
        Args:
            frame: Current BGR frame.
            bbox: Face bounding box.
            track_id: Tracking ID for this face.
            
        Returns:
            Motion score (0.0 = no motion, 1.0 = high motion).
        """
        if track_id not in self._prev_frames:
            # Store current frame for next iteration
            h, w = frame.shape[:2]
            x1 = max(0, int(bbox.x))
            y1 = max(0, int(bbox.y))
            x2 = min(w, int(bbox.x2))
            y2 = min(h, int(bbox.y2))
            
            if x2 > x1 and y2 > y1:
                face_region = frame[y1:y2, x1:x2].copy()
                self._prev_frames[track_id] = face_region
            return 0.0

        try:
            # Get previous frame region
            prev_region = self._prev_frames[track_id]
            
            # Extract current region
            h, w = frame.shape[:2]
            x1 = max(0, int(bbox.x))
            y1 = max(0, int(bbox.y))
            x2 = min(w, int(bbox.x2))
            y2 = min(h, int(bbox.y2))
            
            if x2 <= x1 or y2 <= y1:
                return 0.0
            
            curr_region = frame[y1:y2, x1:x2]
            
            # Resize to match if sizes differ
            if prev_region.shape != curr_region.shape:
                target_size = (curr_region.shape[1], curr_region.shape[0])
                prev_region = cv2.resize(prev_region, target_size)
            
            # Convert to grayscale
            prev_gray = cv2.cvtColor(prev_region, cv2.COLOR_BGR2GRAY)
            curr_gray = cv2.cvtColor(curr_region, cv2.COLOR_BGR2GRAY)
            
            # Compute optical flow (Lucas-Kanade method)
            # Use sparse features for efficiency
            corners = cv2.goodFeaturesToTrack(
                prev_gray,
                maxCorners=50,
                qualityLevel=0.01,
                minDistance=10,
                blockSize=3,
            )
            
            if corners is None or len(corners) < 5:
                # Fallback: simple frame difference
                diff = cv2.absdiff(prev_gray, curr_gray)
                motion_score = np.mean(diff) / 255.0
            else:
                # Compute optical flow
                flow, status, _ = cv2.calcOpticalFlowPyrLK(
                    prev_gray, curr_gray, corners, None
                )
                
                # Compute average motion magnitude
                good_flow = flow[status == 1]
                if len(good_flow) > 0:
                    motion_vectors = good_flow - corners[status == 1]
                    motion_magnitudes = np.linalg.norm(motion_vectors, axis=1)
                    motion_score = np.mean(motion_magnitudes) / max(prev_region.shape[:2])
                    motion_score = np.clip(motion_score * 10, 0.0, 1.0)  # Scale and clamp
                else:
                    motion_score = 0.0
            
            # Update stored frame
            self._prev_frames[track_id] = curr_region.copy()
            
            return float(motion_score)
            
        except Exception as e:
            logger.debug(f"Error computing motion score: {e}")
            return 0.0

    def compute_size_change_score(
        self,
        bbox: BoundingBox,
        score: float,
        track_id: int,
        time: float,
    ) -> float:
        """
        Compute score based on face size and confidence changes.
        
        Faces that are growing or have increasing confidence may be becoming
        more prominent (speaking, moving forward).
        
        Args:
            bbox: Current face bounding box.
            score: Detection confidence.
            track_id: Tracking ID.
            time: Current timestamp.
            
        Returns:
            Size change score (0.0 = no change, 1.0 = significant increase).
        """
        # Store current state
        face_area = bbox.area
        self._face_history[track_id].append((face_area, score, time))
        
        # Keep only recent history (within activity window)
        window_start = time - self.config.face_activity_window
        self._face_history[track_id] = [
            h for h in self._face_history[track_id] if h[2] >= window_start
        ]
        
        if len(self._face_history[track_id]) < 2:
            return 0.0
        
        # Compute trend
        areas = [h[0] for h in self._face_history[track_id]]
        confidences = [h[1] for h in self._face_history[track_id]]
        
        # Area trend (normalized)
        area_trend = (areas[-1] - areas[0]) / (areas[0] + 1e-6)
        
        # Confidence trend
        conf_trend = confidences[-1] - confidences[0]
        
        # Combined score (positive = growing/prominent)
        size_score = (area_trend * 0.7 + conf_trend * 0.3)
        size_score = np.clip(size_score, -1.0, 1.0)
        
        # Normalize to 0-1 (we care about increases)
        return float(max(0.0, size_score))

    def compute_activity_score(
        self,
        frame: np.ndarray,
        detection: Detection,
    ) -> float:
        """
        Compute overall activity score for a face detection.
        
        Combines mouth movement, motion, and size changes.
        
        Args:
            frame: Current BGR frame.
            detection: Face detection.
            
        Returns:
            Activity score (0.0 = inactive, 1.0 = highly active).
        """
        scores = []
        weights = []
        
        # Mouth movement
        if self.config.activity_weight_mouth > 0 and self.config.enable_mouth_detection:
            mouth_score = self.compute_mouth_openness(frame, detection.bbox)
            if mouth_score is not None:
                scores.append(mouth_score)
                weights.append(self.config.activity_weight_mouth)
        
        # Motion
        if self.config.activity_weight_motion > 0:
            motion_score = self.compute_motion_score(
                frame, detection.bbox, detection.track_id
            )
            scores.append(motion_score)
            weights.append(self.config.activity_weight_motion)
        
        # Size change
        if self.config.activity_weight_size_change > 0:
            size_score = self.compute_size_change_score(
                detection.bbox, detection.score, detection.track_id, detection.time
            )
            scores.append(size_score)
            weights.append(self.config.activity_weight_size_change)
        
        if not scores:
            return 0.0
        
        # Weighted average
        total_weight = sum(weights)
        if total_weight == 0:
            return 0.0
        
        activity = sum(s * w for s, w in zip(scores, weights)) / total_weight
        return float(np.clip(activity, 0.0, 1.0))

    def cleanup_track(self, track_id: int):
        """Clean up resources for a track that's no longer active."""
        self._prev_frames.pop(track_id, None)
        self._face_history.pop(track_id, None)

    def close(self):
        """Release resources."""
        if self._face_mesh is not None:
            self._face_mesh.close()
            self._face_mesh = None
        self._prev_frames.clear()
        self._face_history.clear()


class TemporalActivityTracker:
    """
    Track face activity over time windows and manage smooth switching.
    """

    def __init__(self, config: IntelligentCropConfig):
        self.config = config
        self.activity_history: dict[int, list[tuple[float, float]]] = defaultdict(list)
        self.current_face: Optional[int] = None
        self.current_face_start_time: Optional[float] = None

    def update_activity(
        self, track_id: int, activity_score: float, time: float
    ):
        """Update activity score for a track."""
        self.activity_history[track_id].append((time, activity_score))
        
        # Keep only recent history
        window_start = time - self.config.face_activity_window
        self.activity_history[track_id] = [
            h for h in self.activity_history[track_id] if h[0] >= window_start
        ]

    def get_average_activity(self, track_id: int, current_time: float) -> float:
        """Get average activity score for a track over the activity window."""
        if track_id not in self.activity_history:
            return 0.0
        
        window_start = current_time - self.config.face_activity_window
        recent = [
            score for time, score in self.activity_history[track_id]
            if time >= window_start
        ]
        
        if not recent:
            return 0.0
        
        # Apply smoothing
        if len(recent) > 1 and self.config.activity_smoothing_window > 0:
            # Simple exponential moving average
            alpha = 1.0 / (1.0 + len(recent) * self.config.activity_smoothing_window)
            smoothed = recent[0]
            for score in recent[1:]:
                smoothed = alpha * score + (1 - alpha) * smoothed
            return smoothed
        
        return float(np.mean(recent))

    def select_active_face(
        self, available_tracks: list[int], current_time: float
    ) -> Optional[int]:
        """
        Select the most active face, respecting minimum switch duration.
        
        Args:
            available_tracks: List of track IDs currently detected.
            current_time: Current timestamp.
            
        Returns:
            Selected track ID or None.
        """
        if not available_tracks:
            self.current_face = None
            self.current_face_start_time = None
            return None
        
        # Compute average activity for each track
        track_activities = {
            track_id: self.get_average_activity(track_id, current_time)
            for track_id in available_tracks
        }
        
        # Find most active track
        best_track = max(track_activities, key=track_activities.get)
        best_activity = track_activities[best_track]
        
        # Check if we should switch
        if self.current_face is None:
            # No current face, select best
            self.current_face = best_track
            self.current_face_start_time = current_time
            return best_track
        
        # Check minimum switch duration
        if self.current_face_start_time is not None:
            time_since_switch = current_time - self.current_face_start_time
            if time_since_switch < self.config.min_switch_duration:
                # Too soon to switch, keep current
                return self.current_face
        
        # Check if best track is significantly better
        current_activity = track_activities.get(self.current_face, 0.0)
        activity_difference = best_activity - current_activity
        
        # Switch if significantly better (at least 20% improvement)
        if best_track != self.current_face and activity_difference > 0.2:
            self.current_face = best_track
            self.current_face_start_time = current_time
            return best_track
        
        # Keep current face
        return self.current_face

    def cleanup_track(self, track_id: int):
        """Clean up resources for a track."""
        self.activity_history.pop(track_id, None)
        if self.current_face == track_id:
            self.current_face = None
            self.current_face_start_time = None

