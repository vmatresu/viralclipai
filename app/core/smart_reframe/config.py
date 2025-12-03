"""
Configuration for the smart reframe pipeline.
"""

from enum import Enum
from typing import Optional
from pydantic import BaseModel, Field


class DetectorBackend(str, Enum):
    """Available detection backends."""

    MEDIAPIPE = "mediapipe"
    YOLO = "yolo"  # Optional, requires ultralytics


class FallbackPolicy(str, Enum):
    """Policy when no faces are detected."""

    CENTER = "center"  # Center crop
    UPPER_CENTER = "upper_center"  # Upper-center (TikTok style)
    SALIENCY = "saliency"  # Saliency-based
    RULE_OF_THIRDS = "rule_of_thirds"  # Rule of thirds composition


class IntelligentCropConfig(BaseModel):
    """Configuration for the intelligent cropping pipeline."""

    # Analysis settings
    fps_sample: float = Field(
        default=3.0,
        ge=0.5,
        le=30.0,
        description="Frames per second to sample for analysis",
    )
    analysis_resolution: int = Field(
        default=480,
        ge=240,
        le=1080,
        description="Resolution (height) to use for analysis",
    )

    # Shot detection
    shot_threshold: float = Field(
        default=0.4,
        ge=0.1,
        le=1.0,
        description="Histogram difference threshold for shot detection",
    )
    min_shot_duration: float = Field(
        default=0.5, ge=0.1, description="Minimum shot duration in seconds"
    )

    # Face/person detection
    detector_backend: DetectorBackend = Field(
        default=DetectorBackend.MEDIAPIPE, description="Detection backend to use"
    )
    min_detection_confidence: float = Field(
        default=0.5,
        ge=0.1,
        le=1.0,
        description="Minimum confidence for face/person detection",
    )
    min_face_size: float = Field(
        default=0.02,
        ge=0.01,
        le=0.5,
        description="Minimum face size as fraction of frame area",
    )

    # Tracking
    iou_threshold: float = Field(
        default=0.3,
        ge=0.1,
        le=0.9,
        description="IoU threshold for track matching",
    )
    max_track_gap: int = Field(
        default=10,
        ge=1,
        description="Maximum frames to maintain a track without detection",
    )

    # Composition
    headroom_ratio: float = Field(
        default=0.15,
        ge=0.0,
        le=0.5,
        description="Target headroom as fraction of crop height",
    )
    subject_padding: float = Field(
        default=0.2,
        ge=0.0,
        le=0.5,
        description="Padding around subject as fraction of subject size",
    )
    safe_margin: float = Field(
        default=0.05,
        ge=0.0,
        le=0.2,
        description="Minimum margin from crop edge as fraction of crop size",
    )

    # Camera smoothing
    max_pan_speed: float = Field(
        default=200.0,
        ge=50.0,
        description="Maximum virtual camera pan speed in pixels per second",
    )
    max_pan_acceleration: float = Field(
        default=400.0,
        ge=100.0,
        description="Maximum virtual camera acceleration in pixels per second squared",
    )
    smoothing_window: float = Field(
        default=0.5,
        ge=0.1,
        le=2.0,
        description="Smoothing window duration in seconds",
    )

    # Zoom limits
    max_zoom_factor: float = Field(
        default=3.0,
        ge=1.5,
        le=5.0,
        description="Maximum zoom factor relative to source",
    )
    min_zoom_factor: float = Field(
        default=1.0,
        ge=1.0,
        le=2.0,
        description="Minimum zoom factor (1.0 = full frame width)",
    )

    # Fallback behavior
    fallback_policy: FallbackPolicy = Field(
        default=FallbackPolicy.UPPER_CENTER,
        description="Fallback when no faces detected",
    )

    # Multi-subject handling
    prefer_primary_subject: bool = Field(
        default=True,
        description="Prefer following primary subject over group framing",
    )
    group_threshold: float = Field(
        default=0.6,
        ge=0.3,
        le=1.0,
        description="Subjects within this fraction of frame width grouped together",
    )

    # Performance
    num_workers: int = Field(
        default=1, ge=1, le=8, description="Number of worker processes for analysis"
    )
    use_gpu: bool = Field(
        default=False, description="Use GPU acceleration if available"
    )

    # Rendering
    render_preset: str = Field(
        default="veryfast",
        description="FFmpeg x264 preset for rendering",
    )
    render_crf: int = Field(
        default=20,
        ge=0,
        le=51,
        description="FFmpeg CRF quality (lower = better quality)",
    )
    letterbox_blur: bool = Field(
        default=True,
        description="Use blurred letterbox instead of black bars",
    )
    letterbox_blur_sigma: float = Field(
        default=30.0,
        ge=5.0,
        le=100.0,
        description="Blur sigma for letterbox background",
    )

    # Debug
    debug: bool = Field(default=False, description="Enable debug output")
    save_debug_frames: bool = Field(
        default=False, description="Save annotated debug frames"
    )


# Default configurations for common use cases
FAST_CONFIG = IntelligentCropConfig(
    fps_sample=2.0,
    analysis_resolution=360,
    render_preset="ultrafast",
    render_crf=23,
)

QUALITY_CONFIG = IntelligentCropConfig(
    fps_sample=5.0,
    analysis_resolution=720,
    render_preset="slow",
    render_crf=18,
    smoothing_window=0.8,
)

TIKTOK_CONFIG = IntelligentCropConfig(
    fallback_policy=FallbackPolicy.UPPER_CENTER,
    headroom_ratio=0.12,
    subject_padding=0.25,
)
