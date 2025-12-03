"""
Shot/scene detection using color histogram differences.

This module provides fast, robust shot detection by comparing
color histograms between consecutive frames.
"""

import logging
from typing import Generator, Optional

import cv2
import numpy as np

from app.core.smart_reframe.models import Shot
from app.core.smart_reframe.config import IntelligentCropConfig
from app.core.utils.opencv import suppress_ffmpeg_warnings

logger = logging.getLogger(__name__)


class ShotDetector:
    """
    Detect shots/scenes using histogram-based comparison.

    Shots are segments of video between scene changes. This detector
    uses color histogram differences to find cut points.
    """

    def __init__(self, config: IntelligentCropConfig):
        self.config = config
        self.threshold = config.shot_threshold
        self.min_duration = config.min_shot_duration

    def detect_shots(
        self,
        video_path: str,
        time_range: Optional[tuple[float, float]] = None,
    ) -> list[Shot]:
        """
        Detect all shots in a video.

        Args:
            video_path: Path to the video file.
            time_range: Optional (start, end) time range to analyze.

        Returns:
            List of Shot objects representing continuous scenes.
        """
        with suppress_ffmpeg_warnings():
            cap = cv2.VideoCapture(video_path)
        if not cap.isOpened():
            raise RuntimeError(f"Failed to open video: {video_path}")

        try:
            fps = cap.get(cv2.CAP_PROP_FPS)
            total_frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
            duration = total_frames / fps if fps > 0 else 0

            # Apply time range
            start_time = 0.0
            end_time = duration
            if time_range:
                start_time, end_time = time_range
                start_time = max(0, start_time)
                end_time = min(duration, end_time)

            # Calculate frame sampling interval
            sample_interval = max(1, int(fps / self.config.fps_sample))

            # Find shot boundaries
            boundaries = list(
                self._detect_boundaries(
                    cap, fps, start_time, end_time, sample_interval
                )
            )

            # Convert boundaries to shots
            shots = self._boundaries_to_shots(boundaries, start_time, end_time)

            logger.info(f"Detected {len(shots)} shots in {video_path}")
            return shots

        finally:
            cap.release()

    def _detect_boundaries(
        self,
        cap: cv2.VideoCapture,
        fps: float,
        start_time: float,
        end_time: float,
        sample_interval: int,
    ) -> Generator[float, None, None]:
        """
        Yield timestamps where shot boundaries occur.
        """
        start_frame = int(start_time * fps)
        end_frame = int(end_time * fps)

        # Seek to start
        cap.set(cv2.CAP_PROP_POS_FRAMES, start_frame)

        prev_hist = None
        frame_idx = start_frame

        while frame_idx < end_frame:
            ret, frame = cap.read()
            if not ret:
                break

            # Downsample for faster processing
            small = cv2.resize(frame, (160, 90))

            # Compute color histogram
            hist = self._compute_histogram(small)

            if prev_hist is not None:
                diff = self._histogram_difference(prev_hist, hist)
                if diff > self.threshold:
                    timestamp = frame_idx / fps
                    yield timestamp

            prev_hist = hist

            # Skip frames according to sample interval
            frame_idx += sample_interval
            cap.set(cv2.CAP_PROP_POS_FRAMES, frame_idx)

    def _compute_histogram(self, frame: np.ndarray) -> np.ndarray:
        """
        Compute a normalized color histogram for a frame.

        Uses HSV color space for better robustness to lighting changes.
        """
        hsv = cv2.cvtColor(frame, cv2.COLOR_BGR2HSV)

        # Compute histograms for H and S channels
        hist_h = cv2.calcHist([hsv], [0], None, [32], [0, 180])
        hist_s = cv2.calcHist([hsv], [1], None, [32], [0, 256])

        # Combine and normalize
        hist = np.concatenate([hist_h, hist_s]).flatten()
        hist = hist / (hist.sum() + 1e-6)

        return hist

    def _histogram_difference(
        self, hist1: np.ndarray, hist2: np.ndarray
    ) -> float:
        """
        Compute difference between two histograms.

        Uses a combination of correlation and chi-square distance.
        """
        # Chi-square distance
        chi_sq = cv2.compareHist(
            hist1.astype(np.float32),
            hist2.astype(np.float32),
            cv2.HISTCMP_CHISQR,
        )

        # Normalize chi-square to [0, 1] range using sigmoid-like function
        normalized = 1 - np.exp(-chi_sq / 2)

        return normalized

    def _boundaries_to_shots(
        self,
        boundaries: list[float],
        start_time: float,
        end_time: float,
    ) -> list[Shot]:
        """
        Convert shot boundary timestamps to Shot objects.

        Merges very short shots according to min_duration.
        """
        # Add start and end as implicit boundaries
        all_times = [start_time] + sorted(boundaries) + [end_time]

        shots = []
        shot_id = 0

        i = 0
        while i < len(all_times) - 1:
            shot_start = all_times[i]
            shot_end = all_times[i + 1]

            # Merge short shots with the next one
            while (shot_end - shot_start) < self.min_duration and i + 2 < len(
                all_times
            ):
                i += 1
                shot_end = all_times[i + 1]

            if shot_end > shot_start:
                shots.append(
                    Shot(id=shot_id, start_time=shot_start, end_time=shot_end)
                )
                shot_id += 1

            i += 1

        return shots


def detect_shots(
    video_path: str,
    config: Optional[IntelligentCropConfig] = None,
    time_range: Optional[tuple[float, float]] = None,
) -> list[Shot]:
    """
    Convenience function for shot detection.

    Args:
        video_path: Path to the video file.
        config: Configuration options. Uses defaults if not provided.
        time_range: Optional (start, end) time range to analyze.

    Returns:
        List of detected Shot objects.
    """
    if config is None:
        config = IntelligentCropConfig()

    detector = ShotDetector(config)
    return detector.detect_shots(video_path, time_range)
