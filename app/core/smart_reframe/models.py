"""
Data models for the smart reframe pipeline.

All models use Pydantic for serialization and validation.
"""

from enum import Enum
from typing import Optional
from pydantic import BaseModel, Field


class AspectRatio(BaseModel):
    """Target aspect ratio for output video."""

    width: int = Field(..., ge=1, description="Aspect ratio width component")
    height: int = Field(..., ge=1, description="Aspect ratio height component")

    def __hash__(self):
        return hash((self.width, self.height))

    def __eq__(self, other):
        if isinstance(other, AspectRatio):
            return self.width == other.width and self.height == other.height
        return False

    @property
    def ratio(self) -> float:
        """Returns width/height as float."""
        return self.width / self.height

    def __str__(self) -> str:
        return f"{self.width}:{self.height}"

    @classmethod
    def from_string(cls, s: str) -> "AspectRatio":
        """Parse aspect ratio from string like '9:16' or '9x16'."""
        for sep in [":", "x", "/"]:
            if sep in s:
                parts = s.split(sep)
                if len(parts) == 2:
                    return cls(width=int(parts[0]), height=int(parts[1]))
        raise ValueError(f"Invalid aspect ratio format: {s}")


class BoundingBox(BaseModel):
    """Bounding box in pixel coordinates."""

    x: float = Field(..., description="Left edge x-coordinate")
    y: float = Field(..., description="Top edge y-coordinate")
    width: float = Field(..., ge=0, description="Box width")
    height: float = Field(..., ge=0, description="Box height")

    @property
    def cx(self) -> float:
        """Center x-coordinate."""
        return self.x + self.width / 2

    @property
    def cy(self) -> float:
        """Center y-coordinate."""
        return self.y + self.height / 2

    @property
    def x2(self) -> float:
        """Right edge x-coordinate."""
        return self.x + self.width

    @property
    def y2(self) -> float:
        """Bottom edge y-coordinate."""
        return self.y + self.height

    @property
    def area(self) -> float:
        """Box area in pixels."""
        return self.width * self.height

    def iou(self, other: "BoundingBox") -> float:
        """Compute Intersection over Union with another box."""
        x1 = max(self.x, other.x)
        y1 = max(self.y, other.y)
        x2 = min(self.x2, other.x2)
        y2 = min(self.y2, other.y2)

        if x2 <= x1 or y2 <= y1:
            return 0.0

        intersection = (x2 - x1) * (y2 - y1)
        union = self.area + other.area - intersection
        return intersection / union if union > 0 else 0.0

    def pad(self, padding: float) -> "BoundingBox":
        """Return a new box with padding added on all sides."""
        return BoundingBox(
            x=self.x - padding,
            y=self.y - padding,
            width=self.width + 2 * padding,
            height=self.height + 2 * padding,
        )

    def clamp(self, frame_width: int, frame_height: int) -> "BoundingBox":
        """
        Clamp box to frame boundaries while preserving center when possible.
        
        This is important for face detection where we want to maintain
        the center position even when the expanded box exceeds boundaries.
        """
        # Get current center
        center_x = self.cx
        center_y = self.cy
        
        # Clamp center to valid range (with half-width/height margin)
        half_width = self.width / 2
        half_height = self.height / 2
        
        # Clamp center, ensuring box stays within bounds
        # If box is too large, center it in that dimension
        if self.width > frame_width:
            clamped_cx = frame_width / 2
        else:
            clamped_cx = max(half_width, min(center_x, frame_width - half_width))
            
        if self.height > frame_height:
            clamped_cy = frame_height / 2
        else:
            clamped_cy = max(half_height, min(center_y, frame_height - half_height))
        
        # Reconstruct box centered on clamped center
        x = clamped_cx - half_width
        y = clamped_cy - half_height
        
        # Final clamp to ensure box is fully within frame
        x = max(0, min(x, frame_width - self.width))
        y = max(0, min(y, frame_height - self.height))
        
        return BoundingBox(x=x, y=y, width=self.width, height=self.height)

    @classmethod
    def union(cls, boxes: list["BoundingBox"]) -> Optional["BoundingBox"]:
        """Compute bounding box that contains all input boxes."""
        if not boxes:
            return None
        x = min(b.x for b in boxes)
        y = min(b.y for b in boxes)
        x2 = max(b.x2 for b in boxes)
        y2 = max(b.y2 for b in boxes)
        return cls(x=x, y=y, width=x2 - x, height=y2 - y)


class Shot(BaseModel):
    """A continuous shot/scene segment in the video."""

    id: int = Field(..., description="Shot identifier")
    start_time: float = Field(..., ge=0, description="Start time in seconds")
    end_time: float = Field(..., ge=0, description="End time in seconds")

    @property
    def duration(self) -> float:
        """Shot duration in seconds."""
        return self.end_time - self.start_time


class Detection(BaseModel):
    """A detected face or person at a specific time."""

    time: float = Field(..., ge=0, description="Frame timestamp in seconds")
    bbox: BoundingBox = Field(..., description="Bounding box in source coordinates")
    score: float = Field(..., ge=0, le=1, description="Detection confidence")
    track_id: int = Field(..., description="Tracking ID for identity across frames")
    type: str = Field(
        "face", description="Detection type: 'face' or 'person'"
    )  # Literal["face", "person"]

    # Optional landmark data for composition hints
    landmarks: Optional[dict] = Field(
        None, description="Key facial landmarks (eyes, nose, chin, etc.)"
    )


class ShotDetections(BaseModel):
    """All detections within a single shot."""

    shot_id: int = Field(..., description="Associated shot ID")
    detections: list[Detection] = Field(
        default_factory=list, description="List of detections"
    )

    def get_primary_track(self) -> Optional[int]:
        """Find the most prominent track ID based on screen time and size."""
        if not self.detections:
            return None

        track_scores: dict[int, float] = {}
        for det in self.detections:
            score = det.bbox.area * det.score
            track_scores[det.track_id] = track_scores.get(det.track_id, 0) + score

        return max(track_scores, key=lambda t: track_scores[t])


class CameraMode(str, Enum):
    """Virtual camera behavior mode."""

    STATIC = "static"
    TRACKING = "tracking"
    ZOOM = "zoom"


class CameraKeyframe(BaseModel):
    """A keyframe in the virtual camera path."""

    time: float = Field(..., ge=0, description="Time in seconds")
    cx: float = Field(..., description="Center x of focus region")
    cy: float = Field(..., description="Center y of focus region")
    width: float = Field(..., ge=0, description="Focus region width")
    height: float = Field(..., ge=0, description="Focus region height")


class ShotCameraPlan(BaseModel):
    """Camera movement plan for a single shot."""

    shot_id: int = Field(..., description="Associated shot ID")
    mode: CameraMode = Field(..., description="Camera behavior mode")
    keyframes: list[CameraKeyframe] = Field(
        default_factory=list, description="Smoothed camera keyframes"
    )


class CropWindow(BaseModel):
    """Final crop window for a specific time."""

    time: float = Field(..., ge=0, description="Frame timestamp in seconds")
    x: int = Field(..., ge=0, description="Left edge x-coordinate")
    y: int = Field(..., ge=0, description="Top edge y-coordinate")
    width: int = Field(..., ge=1, description="Crop window width")
    height: int = Field(..., ge=1, description="Crop window height")


class ShotCropPlan(BaseModel):
    """Crop windows for a single shot and aspect ratio."""

    shot_id: int = Field(..., description="Associated shot ID")
    aspect_ratio: AspectRatio = Field(..., description="Target aspect ratio")
    crop_windows: list[CropWindow] = Field(
        default_factory=list, description="Crop windows at sampled times"
    )


class VideoMeta(BaseModel):
    """Metadata about the source video."""

    input_path: str = Field(..., description="Path to input video")
    duration: float = Field(..., ge=0, description="Video duration in seconds")
    width: int = Field(..., ge=1, description="Frame width in pixels")
    height: int = Field(..., ge=1, description="Frame height in pixels")
    fps: float = Field(..., ge=0, description="Video frame rate")


class CropPlan(BaseModel):
    """
    Complete crop plan for a video.

    This contains all analysis results and can be serialized to JSON
    for caching, debugging, or re-rendering at different resolutions.
    """

    video: VideoMeta = Field(..., description="Source video metadata")
    target_aspect_ratios: list[AspectRatio] = Field(
        ..., description="Target aspect ratios"
    )
    shots: list[Shot] = Field(default_factory=list, description="Detected shots")
    shot_detections: list[ShotDetections] = Field(
        default_factory=list, description="Detections per shot"
    )
    shot_camera_plans: list[ShotCameraPlan] = Field(
        default_factory=list, description="Camera plans per shot"
    )
    shot_crop_plans: list[ShotCropPlan] = Field(
        default_factory=list, description="Crop plans per shot and aspect ratio"
    )

    def to_json_file(self, path: str) -> None:
        """Serialize crop plan to JSON file."""
        import json
        from pathlib import Path

        Path(path).write_text(self.model_dump_json(indent=2), encoding="utf-8")

    @classmethod
    def from_json_file(cls, path: str) -> "CropPlan":
        """Load crop plan from JSON file."""
        import json
        from pathlib import Path

        data = json.loads(Path(path).read_text(encoding="utf-8"))
        return cls.model_validate(data)
