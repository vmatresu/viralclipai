//! Data models for intelligent cropping pipeline.
//!
//! These models mirror the Python implementation's data structures.

use serde::{Deserialize, Serialize};

/// Bounding box in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Left edge x-coordinate
    pub x: f64,
    /// Top edge y-coordinate
    pub y: f64,
    /// Box width
    pub width: f64,
    /// Box height
    pub height: f64,
}

impl BoundingBox {
    /// Create a new bounding box.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// Center x-coordinate.
    #[inline]
    pub fn cx(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Center y-coordinate.
    #[inline]
    pub fn cy(&self) -> f64 {
        self.y + self.height / 2.0
    }

    /// Right edge x-coordinate.
    #[inline]
    pub fn x2(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge y-coordinate.
    #[inline]
    pub fn y2(&self) -> f64 {
        self.y + self.height
    }

    /// Box area in pixels.
    #[inline]
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Compute Intersection over Union with another box.
    pub fn iou(&self, other: &BoundingBox) -> f64 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.x2().min(other.x2());
        let y2 = self.y2().min(other.y2());

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let union = self.area() + other.area() - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }

    /// Return a new box with padding added on all sides.
    pub fn pad(&self, padding: f64) -> BoundingBox {
        BoundingBox {
            x: self.x - padding,
            y: self.y - padding,
            width: self.width + 2.0 * padding,
            height: self.height + 2.0 * padding,
        }
    }

    /// Clamp box to frame boundaries while preserving center when possible.
    pub fn clamp(&self, frame_width: u32, frame_height: u32) -> BoundingBox {
        let frame_width = frame_width as f64;
        let frame_height = frame_height as f64;

        let center_x = self.cx();
        let center_y = self.cy();
        let half_width = self.width / 2.0;
        let half_height = self.height / 2.0;

        // Clamp center, ensuring box stays within bounds
        let clamped_cx = if self.width > frame_width {
            frame_width / 2.0
        } else {
            center_x.max(half_width).min(frame_width - half_width)
        };

        let clamped_cy = if self.height > frame_height {
            frame_height / 2.0
        } else {
            center_y.max(half_height).min(frame_height - half_height)
        };

        // Reconstruct box centered on clamped center
        let mut x = clamped_cx - half_width;
        let mut y = clamped_cy - half_height;

        // Final clamp to ensure box is fully within frame
        x = x.max(0.0).min(frame_width - self.width);
        y = y.max(0.0).min(frame_height - self.height);

        BoundingBox {
            x,
            y,
            width: self.width,
            height: self.height,
        }
    }

    /// Compute bounding box that contains all input boxes.
    pub fn union(boxes: &[BoundingBox]) -> Option<BoundingBox> {
        if boxes.is_empty() {
            return None;
        }

        let x = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
        let y = boxes.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
        let x2 = boxes.iter().map(|b| b.x2()).fold(f64::NEG_INFINITY, f64::max);
        let y2 = boxes.iter().map(|b| b.y2()).fold(f64::NEG_INFINITY, f64::max);

        Some(BoundingBox {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        })
    }
}

/// A face detection at a specific time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Timestamp in seconds
    pub time: f64,
    /// Bounding box of the detection
    pub bbox: BoundingBox,
    /// Detection confidence score (0.0-1.0)
    pub score: f64,
    /// Track ID for identity persistence
    pub track_id: u32,
    /// Optional mouth openness score from face mesh (SpeakerAware tiers)
    pub mouth_openness: Option<f64>,
}

impl Detection {
    /// Create a new detection.
    pub fn new(time: f64, bbox: BoundingBox, score: f64, track_id: u32) -> Self {
        Self {
            time,
            bbox,
            score,
            track_id,
            mouth_openness: None,
        }
    }

    /// Create with mouth openness.
    pub fn with_mouth(
        time: f64,
        bbox: BoundingBox,
        score: f64,
        track_id: u32,
        mouth_openness: Option<f64>,
    ) -> Self {
        Self {
            time,
            bbox,
            score,
            track_id,
            mouth_openness,
        }
    }
}

/// Detections for a time frame.
pub type FrameDetections = Vec<Detection>;

/// Camera keyframe representing virtual camera position and size.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraKeyframe {
    /// Timestamp in seconds
    pub time: f64,
    /// Center x-coordinate
    pub cx: f64,
    /// Center y-coordinate
    pub cy: f64,
    /// Focus region width
    pub width: f64,
    /// Focus region height
    pub height: f64,
}

impl CameraKeyframe {
    /// Create a new camera keyframe.
    pub fn new(time: f64, cx: f64, cy: f64, width: f64, height: f64) -> Self {
        Self {
            time,
            cx,
            cy,
            width,
            height,
        }
    }

    /// Create a centered keyframe.
    pub fn centered(time: f64, frame_width: u32, frame_height: u32) -> Self {
        let width = frame_width as f64 * 0.6;
        let height = frame_height as f64 * 0.6;
        Self {
            time,
            cx: frame_width as f64 / 2.0,
            cy: frame_height as f64 / 2.0,
            width,
            height,
        }
    }
}

/// Camera mode classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraMode {
    /// Static camera - no movement
    Static,
    /// Tracking camera - following subject
    Tracking,
    /// Zooming camera - changing scale
    Zoom,
}

/// Crop window for FFmpeg rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CropWindow {
    /// Timestamp in seconds
    pub time: f64,
    /// Left edge x-coordinate (integer for FFmpeg)
    pub x: i32,
    /// Top edge y-coordinate (integer for FFmpeg)
    pub y: i32,
    /// Crop width
    pub width: i32,
    /// Crop height
    pub height: i32,
}

impl CropWindow {
    /// Create a new crop window.
    pub fn new(time: f64, x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            time,
            x,
            y,
            width,
            height,
        }
    }

    /// Linear interpolation between two crop windows.
    pub fn lerp(a: &CropWindow, b: &CropWindow, t: f64) -> CropWindow {
        CropWindow {
            time: a.time + t * (b.time - a.time),
            x: (a.x as f64 + t * (b.x - a.x) as f64).round() as i32,
            y: (a.y as f64 + t * (b.y - a.y) as f64).round() as i32,
            width: (a.width as f64 + t * (b.width - a.width) as f64).round() as i32,
            height: (a.height as f64 + t * (b.height - a.height) as f64).round() as i32,
        }
    }
}

/// Target aspect ratio for output video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AspectRatio {
    /// Width component
    pub width: u32,
    /// Height component
    pub height: u32,
}

impl AspectRatio {
    /// Create a new aspect ratio.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Returns width/height as float.
    pub fn ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }

    /// Portrait 9:16 (TikTok, Instagram Reels)
    pub const PORTRAIT: AspectRatio = AspectRatio { width: 9, height: 16 };

    /// Square 1:1 (Instagram)
    pub const SQUARE: AspectRatio = AspectRatio { width: 1, height: 1 };

    /// Landscape 16:9 (YouTube)
    pub const LANDSCAPE: AspectRatio = AspectRatio { width: 16, height: 9 };
}

impl std::fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_iou() {
        let box1 = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let box2 = BoundingBox::new(50.0, 50.0, 100.0, 100.0);

        let iou = box1.iou(&box2);
        // Intersection: 50x50 = 2500
        // Union: 10000 + 10000 - 2500 = 17500
        // IoU: 2500/17500 = 0.1428...
        assert!((iou - 0.1428).abs() < 0.01);
    }

    #[test]
    fn test_bounding_box_no_overlap() {
        let box1 = BoundingBox::new(0.0, 0.0, 50.0, 50.0);
        let box2 = BoundingBox::new(100.0, 100.0, 50.0, 50.0);

        assert_eq!(box1.iou(&box2), 0.0);
    }

    #[test]
    fn test_bounding_box_union() {
        let boxes = vec![
            BoundingBox::new(0.0, 0.0, 50.0, 50.0),
            BoundingBox::new(100.0, 100.0, 50.0, 50.0),
        ];

        let union = BoundingBox::union(&boxes).unwrap();
        assert_eq!(union.x, 0.0);
        assert_eq!(union.y, 0.0);
        assert_eq!(union.width, 150.0);
        assert_eq!(union.height, 150.0);
    }

    #[test]
    fn test_crop_window_lerp() {
        let a = CropWindow::new(0.0, 0, 0, 100, 100);
        let b = CropWindow::new(1.0, 100, 100, 200, 200);

        let mid = CropWindow::lerp(&a, &b, 0.5);
        assert_eq!(mid.x, 50);
        assert_eq!(mid.y, 50);
        assert_eq!(mid.width, 150);
        assert_eq!(mid.height, 150);
    }
}
