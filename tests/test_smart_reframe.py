"""
Tests for the smart_reframe module.

Run with: pytest tests/test_smart_reframe.py -v
"""

import pytest
import tempfile
from pathlib import Path
from unittest.mock import Mock, patch, MagicMock
import numpy as np

# Import models and config
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
from app.core.smart_reframe.config import (
    IntelligentCropConfig,
    DetectorBackend,
    FallbackPolicy,
    FAST_CONFIG,
    QUALITY_CONFIG,
    TIKTOK_CONFIG,
)


class TestAspectRatio:
    """Tests for AspectRatio model."""

    def test_ratio_calculation(self):
        ar = AspectRatio(width=9, height=16)
        assert ar.ratio == 9 / 16

    def test_from_string_colon(self):
        ar = AspectRatio.from_string("9:16")
        assert ar.width == 9
        assert ar.height == 16

    def test_from_string_x(self):
        ar = AspectRatio.from_string("4x5")
        assert ar.width == 4
        assert ar.height == 5

    def test_from_string_slash(self):
        ar = AspectRatio.from_string("1/1")
        assert ar.width == 1
        assert ar.height == 1

    def test_str_representation(self):
        ar = AspectRatio(width=9, height=16)
        assert str(ar) == "9:16"

    def test_equality(self):
        ar1 = AspectRatio(width=9, height=16)
        ar2 = AspectRatio(width=9, height=16)
        ar3 = AspectRatio(width=4, height=5)
        assert ar1 == ar2
        assert ar1 != ar3

    def test_hash(self):
        ar1 = AspectRatio(width=9, height=16)
        ar2 = AspectRatio(width=9, height=16)
        assert hash(ar1) == hash(ar2)
        # Should be usable in sets
        s = {ar1, ar2}
        assert len(s) == 1


class TestBoundingBox:
    """Tests for BoundingBox model."""

    def test_center_coordinates(self):
        box = BoundingBox(x=100, y=200, width=50, height=100)
        assert box.cx == 125
        assert box.cy == 250

    def test_edges(self):
        box = BoundingBox(x=100, y=200, width=50, height=100)
        assert box.x2 == 150
        assert box.y2 == 300

    def test_area(self):
        box = BoundingBox(x=0, y=0, width=100, height=50)
        assert box.area == 5000

    def test_iou_no_overlap(self):
        box1 = BoundingBox(x=0, y=0, width=50, height=50)
        box2 = BoundingBox(x=100, y=100, width=50, height=50)
        assert box1.iou(box2) == 0.0

    def test_iou_full_overlap(self):
        box = BoundingBox(x=0, y=0, width=50, height=50)
        assert box.iou(box) == 1.0

    def test_iou_partial_overlap(self):
        box1 = BoundingBox(x=0, y=0, width=100, height=100)
        box2 = BoundingBox(x=50, y=50, width=100, height=100)
        # Intersection: 50x50 = 2500
        # Union: 10000 + 10000 - 2500 = 17500
        expected_iou = 2500 / 17500
        assert abs(box1.iou(box2) - expected_iou) < 0.001

    def test_pad(self):
        box = BoundingBox(x=100, y=100, width=50, height=50)
        padded = box.pad(10)
        assert padded.x == 90
        assert padded.y == 90
        assert padded.width == 70
        assert padded.height == 70

    def test_clamp(self):
        box = BoundingBox(x=-10, y=-10, width=100, height=100)
        clamped = box.clamp(80, 80)
        assert clamped.x == 0
        assert clamped.y == 0
        assert clamped.width == 80
        assert clamped.height == 80

    def test_union(self):
        boxes = [
            BoundingBox(x=0, y=0, width=50, height=50),
            BoundingBox(x=100, y=100, width=50, height=50),
        ]
        union = BoundingBox.union(boxes)
        assert union.x == 0
        assert union.y == 0
        assert union.width == 150
        assert union.height == 150

    def test_union_empty(self):
        assert BoundingBox.union([]) is None


class TestShot:
    """Tests for Shot model."""

    def test_duration(self):
        shot = Shot(id=0, start_time=10.0, end_time=25.0)
        assert shot.duration == 15.0


class TestShotDetections:
    """Tests for ShotDetections model."""

    def test_get_primary_track_single(self):
        dets = ShotDetections(
            shot_id=0,
            detections=[
                Detection(
                    time=0.0,
                    bbox=BoundingBox(x=0, y=0, width=100, height=100),
                    score=0.9,
                    track_id=1,
                    type="face",
                ),
            ],
        )
        assert dets.get_primary_track() == 1

    def test_get_primary_track_multiple(self):
        dets = ShotDetections(
            shot_id=0,
            detections=[
                # Track 1: small face
                Detection(
                    time=0.0,
                    bbox=BoundingBox(x=0, y=0, width=50, height=50),
                    score=0.9,
                    track_id=1,
                    type="face",
                ),
                # Track 2: large face (should be primary)
                Detection(
                    time=0.0,
                    bbox=BoundingBox(x=0, y=0, width=200, height=200),
                    score=0.9,
                    track_id=2,
                    type="face",
                ),
            ],
        )
        assert dets.get_primary_track() == 2

    def test_get_primary_track_empty(self):
        dets = ShotDetections(shot_id=0, detections=[])
        assert dets.get_primary_track() is None


class TestConfig:
    """Tests for configuration."""

    def test_default_config(self):
        config = IntelligentCropConfig()
        assert config.fps_sample == 3.0
        assert config.detector_backend == DetectorBackend.MEDIAPIPE
        assert config.fallback_policy == FallbackPolicy.UPPER_CENTER

    def test_fast_config(self):
        assert FAST_CONFIG.fps_sample == 2.0
        assert FAST_CONFIG.render_preset == "ultrafast"

    def test_quality_config(self):
        assert QUALITY_CONFIG.fps_sample == 5.0
        assert QUALITY_CONFIG.render_preset == "slow"

    def test_tiktok_config(self):
        assert TIKTOK_CONFIG.fallback_policy == FallbackPolicy.UPPER_CENTER


class TestCropPlan:
    """Tests for CropPlan serialization."""

    def test_to_json_and_back(self):
        plan = CropPlan(
            video=VideoMeta(
                input_path="/test/video.mp4",
                duration=60.0,
                width=1920,
                height=1080,
                fps=30.0,
            ),
            target_aspect_ratios=[AspectRatio(width=9, height=16)],
            shots=[Shot(id=0, start_time=0.0, end_time=60.0)],
            shot_detections=[],
            shot_camera_plans=[],
            shot_crop_plans=[],
        )

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            plan.to_json_file(f.name)
            loaded = CropPlan.from_json_file(f.name)

        assert loaded.video.input_path == plan.video.input_path
        assert loaded.video.duration == plan.video.duration
        assert len(loaded.target_aspect_ratios) == 1
        assert loaded.target_aspect_ratios[0].width == 9


class TestSmoother:
    """Tests for camera path smoothing."""

    def test_moving_average(self):
        from app.core.smart_reframe.smoother import CameraSmoother

        config = IntelligentCropConfig()
        smoother = CameraSmoother(config, fps=30.0)

        data = np.array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], dtype=float)
        smoothed = smoother._moving_average(data, window=3)

        # Should have same length
        assert len(smoothed) == len(data)
        # Middle values should be averaged
        assert smoothed[1] == pytest.approx(2.0, rel=0.1)


class TestCropPlanner:
    """Tests for crop window computation."""

    def test_narrow_crop(self):
        from app.core.smart_reframe.crop_planner import CropPlanner
        from app.core.smart_reframe.models import CameraKeyframe

        config = IntelligentCropConfig()
        planner = CropPlanner(config, frame_width=1920, frame_height=1080)

        keyframe = CameraKeyframe(
            time=0.0,
            cx=960,
            cy=540,
            width=400,
            height=400,
        )

        crop = planner._keyframe_to_crop(keyframe, AspectRatio(9, 16))

        # Should produce a vertical crop
        assert crop.height > crop.width
        # Aspect ratio should match
        actual_ratio = crop.width / crop.height
        expected_ratio = 9 / 16
        assert abs(actual_ratio - expected_ratio) < 0.01


class TestSaliency:
    """Tests for saliency fallback."""

    def test_center_focus(self):
        from app.core.smart_reframe.saliency import SaliencyEstimator

        estimator = SaliencyEstimator()
        focus = estimator._center_focus(1920, 1080)

        # Should be centered
        assert abs(focus.cx - 960) < 100
        assert abs(focus.cy - 540) < 100

    def test_upper_center_focus(self):
        from app.core.smart_reframe.saliency import SaliencyEstimator

        estimator = SaliencyEstimator()
        focus = estimator._upper_center_focus(1920, 1080)

        # Should be centered horizontally
        assert abs(focus.cx - 960) < 100
        # Should be biased upward
        assert focus.cy < 540


class TestSimpleTracker:
    """Tests for IoU-based tracking."""

    def test_new_track(self):
        from app.core.smart_reframe.content_analyzer import SimpleTracker

        tracker = SimpleTracker()
        detections = [
            (BoundingBox(x=100, y=100, width=50, height=50), 0.9),
        ]

        tracked = tracker.update(detections)
        assert len(tracked) == 1
        assert tracked[0][0] == 0  # First track ID

    def test_track_continuation(self):
        from app.core.smart_reframe.content_analyzer import SimpleTracker

        tracker = SimpleTracker(iou_threshold=0.3)

        # Frame 1
        tracked1 = tracker.update([
            (BoundingBox(x=100, y=100, width=50, height=50), 0.9),
        ])

        # Frame 2 - box moved slightly
        tracked2 = tracker.update([
            (BoundingBox(x=110, y=105, width=50, height=50), 0.9),
        ])

        # Should maintain same track ID
        assert tracked1[0][0] == tracked2[0][0]

    def test_new_track_on_distant_detection(self):
        from app.core.smart_reframe.content_analyzer import SimpleTracker

        tracker = SimpleTracker(iou_threshold=0.3)

        # Frame 1
        tracked1 = tracker.update([
            (BoundingBox(x=100, y=100, width=50, height=50), 0.9),
        ])

        # Frame 2 - completely different location
        tracked2 = tracker.update([
            (BoundingBox(x=500, y=500, width=50, height=50), 0.9),
        ])

        # Should create new track
        assert tracked1[0][0] != tracked2[0][0]


# Integration tests (require OpenCV and MediaPipe)
class TestIntegration:
    """Integration tests that require actual dependencies."""

    @pytest.mark.skipif(
        not pytest.importorskip("cv2", reason="OpenCV not installed"),
        reason="OpenCV required"
    )
    def test_shot_detector_init(self):
        from app.core.smart_reframe.shot_detector import ShotDetector

        config = IntelligentCropConfig()
        detector = ShotDetector(config)
        assert detector.threshold == config.shot_threshold

    @pytest.mark.skipif(
        not pytest.importorskip("cv2", reason="OpenCV not installed"),
        reason="OpenCV required"
    )
    def test_histogram_computation(self):
        from app.core.smart_reframe.shot_detector import ShotDetector

        config = IntelligentCropConfig()
        detector = ShotDetector(config)

        # Create a simple test frame
        frame = np.zeros((90, 160, 3), dtype=np.uint8)
        frame[:, :, 0] = 128  # Blue channel

        hist = detector._compute_histogram(frame)
        assert hist.shape == (64,)  # 32 + 32 bins
        assert hist.sum() == pytest.approx(1.0, rel=0.01)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
