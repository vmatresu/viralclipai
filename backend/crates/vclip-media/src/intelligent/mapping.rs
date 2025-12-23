//! Coordinate Mapping for Fixed-Resolution Letterbox Inference
//!
//! Provides deterministic, reversible coordinate mapping between:
//! - **Raw space**: Original video frame dimensions
//! - **Inference space**: Fixed-size letterboxed canvas for neural network input
//!
//! # Key Concepts
//!
//! ## Letterboxing
//! To maintain aspect ratio, the raw frame is scaled to fit within the inference
//! canvas, with padding added to fill the remaining space.
//!
//! ## Padding Value
//! The padding color must match model expectations for accurate edge detection:
//! - YuNet: 0 (black)
//! - Some models: 128 (gray mid-point)
//!
//! ## Inverse Mapping
//! Detections in inference space must be mapped back to raw space for output.
//! Formula: `x_raw = clamp((x_inf - pad_left) / scale, 0, W_raw)`
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::mapping::MappingMeta;
//!
//! // Create mapping for 1080p video to 960x540 inference
//! let meta = MappingMeta::for_yunet(1920, 1080, 960, 540);
//!
//! // Map a detection back to raw coordinates
//! let (x_raw, y_raw) = meta.map_point(480.0, 270.0);
//! ```

use super::models::BoundingBox;

/// Default inference canvas width for YuNet.
pub const DEFAULT_INF_WIDTH: u32 = 960;

/// Default inference canvas height for YuNet.
pub const DEFAULT_INF_HEIGHT: u32 = 540;

/// Coordinate mapping metadata for letterbox transformations.
///
/// Stores all information needed to transform coordinates between
/// raw video space and fixed inference canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MappingMeta {
    /// Original frame width in pixels
    pub raw_width: u32,
    /// Original frame height in pixels
    pub raw_height: u32,
    /// Inference canvas width in pixels
    pub inf_width: u32,
    /// Inference canvas height in pixels
    pub inf_height: u32,
    /// Scale factor applied to raw frame (min of x/y scales)
    pub scale: f64,
    /// Left padding in inference space (pixels)
    pub pad_left: i32,
    /// Top padding in inference space (pixels)
    pub pad_top: i32,
    /// Scaled frame width before padding
    pub scaled_width: i32,
    /// Scaled frame height before padding
    pub scaled_height: i32,
    /// Padding fill value (0-255)
    /// YuNet expects 0 (black), other models may expect 128 (gray)
    pub padding_value: u8,
}

impl MappingMeta {
    /// Compute letterbox mapping for given dimensions.
    ///
    /// Uses aspect-preserving scaling with centered padding.
    ///
    /// # Arguments
    /// * `raw_w` - Original frame width
    /// * `raw_h` - Original frame height
    /// * `inf_w` - Inference canvas width
    /// * `inf_h` - Inference canvas height
    pub fn compute(raw_w: u32, raw_h: u32, inf_w: u32, inf_h: u32) -> Self {
        // Compute scale factor (aspect-preserving)
        let scale_x = inf_w as f64 / raw_w as f64;
        let scale_y = inf_h as f64 / raw_h as f64;
        let scale = scale_x.min(scale_y);

        // Compute scaled dimensions
        let scaled_w = (raw_w as f64 * scale).round() as i32;
        let scaled_h = (raw_h as f64 * scale).round() as i32;

        // Compute centered padding
        let pad_left = (inf_w as i32 - scaled_w) / 2;
        let pad_top = (inf_h as i32 - scaled_h) / 2;

        Self {
            raw_width: raw_w,
            raw_height: raw_h,
            inf_width: inf_w,
            inf_height: inf_h,
            scale,
            pad_left,
            pad_top,
            scaled_width: scaled_w,
            scaled_height: scaled_h,
            padding_value: 0, // Default to black
        }
    }

    /// Create mapping for YuNet with correct padding value.
    ///
    /// YuNet expects black (0) padding for optimal edge detection accuracy.
    pub fn for_yunet(raw_w: u32, raw_h: u32, inf_w: u32, inf_h: u32) -> Self {
        let mut meta = Self::compute(raw_w, raw_h, inf_w, inf_h);
        meta.padding_value = 0; // YuNet: black padding
        meta
    }

    /// Create mapping optimized for common YouTube resolutions.
    ///
    /// Uses optimal inference sizes for 16:9 content to minimize padding.
    pub fn for_youtube(raw_w: u32, raw_h: u32) -> Self {
        // Select inference size based on input resolution
        let (inf_w, inf_h) = match (raw_w, raw_h) {
            // 1080p: Perfect 2x scale to 960x540
            (1920, 1080) => (960, 540),
            // 720p: Perfect 2x scale to 640x360
            (1280, 720) => (640, 360),
            // 4K: 4x scale to 960x540
            (3840, 2160) => (960, 540),
            // 1440p: 4x scale to 640x360 (better for small faces)
            (2560, 1440) => (640, 360),
            // Default for other resolutions
            _ => (DEFAULT_INF_WIDTH, DEFAULT_INF_HEIGHT),
        };

        Self::for_yunet(raw_w, raw_h, inf_w, inf_h)
    }

    /// Create with default inference size (960x540).
    pub fn with_defaults(raw_w: u32, raw_h: u32) -> Self {
        Self::for_yunet(raw_w, raw_h, DEFAULT_INF_WIDTH, DEFAULT_INF_HEIGHT)
    }

    /// Map a point from inference space to raw space.
    ///
    /// Formula: `x_raw = clamp((x_inf - pad_left) / scale, 0, W_raw - 1)`
    ///
    /// # Arguments
    /// * `x_inf` - X coordinate in inference space
    /// * `y_inf` - Y coordinate in inference space
    ///
    /// # Returns
    /// (x_raw, y_raw) tuple clamped to raw frame bounds
    #[inline]
    pub fn map_point(&self, x_inf: f64, y_inf: f64) -> (f64, f64) {
        let x_raw = (x_inf - self.pad_left as f64) / self.scale;
        let y_raw = (y_inf - self.pad_top as f64) / self.scale;

        (
            x_raw.clamp(0.0, self.raw_width as f64 - 1.0),
            y_raw.clamp(0.0, self.raw_height as f64 - 1.0),
        )
    }

    /// Map a point from raw space to inference space.
    ///
    /// Formula: `x_inf = x_raw * scale + pad_left`
    ///
    /// # Arguments
    /// * `x_raw` - X coordinate in raw space
    /// * `y_raw` - Y coordinate in raw space
    ///
    /// # Returns
    /// (x_inf, y_inf) tuple
    #[inline]
    pub fn map_point_to_inf(&self, x_raw: f64, y_raw: f64) -> (f64, f64) {
        let x_inf = x_raw * self.scale + self.pad_left as f64;
        let y_inf = y_raw * self.scale + self.pad_top as f64;
        (x_inf, y_inf)
    }

    /// Map a bounding box from inference space to raw space.
    ///
    /// Maps both corners and computes the resulting rectangle,
    /// clamped to raw frame bounds.
    pub fn map_rect(&self, bbox_inf: &BoundingBox) -> BoundingBox {
        // Map top-left corner
        let (x1, y1) = self.map_point(bbox_inf.x, bbox_inf.y);
        // Map bottom-right corner
        let (x2, y2) = self.map_point(bbox_inf.x + bbox_inf.width, bbox_inf.y + bbox_inf.height);

        // Clamp to frame bounds
        let x1 = x1.clamp(0.0, self.raw_width as f64);
        let y1 = y1.clamp(0.0, self.raw_height as f64);
        let x2 = x2.clamp(0.0, self.raw_width as f64);
        let y2 = y2.clamp(0.0, self.raw_height as f64);

        BoundingBox::new(x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0))
    }

    /// Map a bounding box from raw space to inference space.
    pub fn map_rect_to_inf(&self, bbox_raw: &BoundingBox) -> BoundingBox {
        let (x1, y1) = self.map_point_to_inf(bbox_raw.x, bbox_raw.y);
        let (x2, y2) =
            self.map_point_to_inf(bbox_raw.x + bbox_raw.width, bbox_raw.y + bbox_raw.height);

        BoundingBox::new(x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0))
    }

    /// Normalize coordinates to [0, 1] range based on raw dimensions.
    pub fn normalize(&self, bbox: &BoundingBox) -> NormalizedBBox {
        NormalizedBBox {
            x: bbox.x / self.raw_width as f64,
            y: bbox.y / self.raw_height as f64,
            w: bbox.width / self.raw_width as f64,
            h: bbox.height / self.raw_height as f64,
        }
    }

    /// Convert normalized coordinates back to raw pixel coordinates.
    pub fn denormalize(&self, bbox_norm: &NormalizedBBox) -> BoundingBox {
        BoundingBox::new(
            bbox_norm.x * self.raw_width as f64,
            bbox_norm.y * self.raw_height as f64,
            bbox_norm.w * self.raw_width as f64,
            bbox_norm.h * self.raw_height as f64,
        )
    }

    /// Check if a point in inference space is within the active (non-padded) area.
    #[inline]
    pub fn is_in_active_area(&self, x_inf: f64, y_inf: f64) -> bool {
        x_inf >= self.pad_left as f64
            && x_inf < (self.pad_left + self.scaled_width) as f64
            && y_inf >= self.pad_top as f64
            && y_inf < (self.pad_top + self.scaled_height) as f64
    }

    /// Get the active (non-padded) region in inference space.
    pub fn active_region(&self) -> (i32, i32, i32, i32) {
        (
            self.pad_left,
            self.pad_top,
            self.scaled_width,
            self.scaled_height,
        )
    }

    /// Get padding amounts (left, top, right, bottom).
    pub fn padding(&self) -> (i32, i32, i32, i32) {
        let pad_right = self.inf_width as i32 - self.scaled_width - self.pad_left;
        let pad_bottom = self.inf_height as i32 - self.scaled_height - self.pad_top;
        (self.pad_left, self.pad_top, pad_right, pad_bottom)
    }
}

/// Normalized bounding box with coordinates in [0, 1] range.
///
/// Resolution-independent representation for storage and comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedBBox {
    /// X coordinate (0 = left, 1 = right)
    pub x: f64,
    /// Y coordinate (0 = top, 1 = bottom)
    pub y: f64,
    /// Width as fraction of frame width
    pub w: f64,
    /// Height as fraction of frame height
    pub h: f64,
}

impl NormalizedBBox {
    /// Create a new normalized bounding box.
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// Create from raw coordinates and frame dimensions.
    pub fn from_raw(bbox: &BoundingBox, frame_width: u32, frame_height: u32) -> Self {
        Self {
            x: bbox.x / frame_width as f64,
            y: bbox.y / frame_height as f64,
            w: bbox.width / frame_width as f64,
            h: bbox.height / frame_height as f64,
        }
    }

    /// Convert to raw pixel coordinates for given frame dimensions.
    pub fn to_raw(&self, frame_width: u32, frame_height: u32) -> BoundingBox {
        BoundingBox::new(
            self.x * frame_width as f64,
            self.y * frame_height as f64,
            self.w * frame_width as f64,
            self.h * frame_height as f64,
        )
    }

    /// Get center point in normalized coordinates.
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Compute IoU (Intersection over Union) with another normalized bbox.
    pub fn iou(&self, other: &NormalizedBBox) -> f64 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);

        let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let union = self.w * self.h + other.w * other.h - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1080p_to_960x540_perfect_scale() {
        // 1920x1080 -> 960x540 is exact 2x scale, no padding needed
        let meta = MappingMeta::compute(1920, 1080, 960, 540);

        assert!((meta.scale - 0.5).abs() < 1e-10);
        assert_eq!(meta.pad_left, 0);
        assert_eq!(meta.pad_top, 0);
        assert_eq!(meta.scaled_width, 960);
        assert_eq!(meta.scaled_height, 540);
    }

    #[test]
    fn test_720p_to_960x540_with_padding() {
        // 1280x720 -> needs letterboxing to fit 960x540
        let meta = MappingMeta::compute(1280, 720, 960, 540);

        // 1280/960 = 1.333, 720/540 = 1.333
        // Scale = min(960/1280, 540/720) = 0.75
        assert!((meta.scale - 0.75).abs() < 1e-10);
        assert_eq!(meta.scaled_width, 960);
        assert_eq!(meta.scaled_height, 540);
        assert_eq!(meta.pad_left, 0);
        assert_eq!(meta.pad_top, 0);
    }

    #[test]
    fn test_4x3_source_letterboxing() {
        // 640x480 (4:3) -> 960x540 (16:9) needs pillarboxing
        let meta = MappingMeta::compute(640, 480, 960, 540);

        // Scale limited by height: 540/480 = 1.125
        // Scaled width: 640 * 1.125 = 720
        // Left padding: (960 - 720) / 2 = 120
        assert!((meta.scale - 1.125).abs() < 1e-10);
        assert_eq!(meta.scaled_width, 720);
        assert_eq!(meta.scaled_height, 540);
        assert_eq!(meta.pad_left, 120);
        assert_eq!(meta.pad_top, 0);
    }

    #[test]
    fn test_round_trip_mapping() {
        let meta = MappingMeta::compute(1920, 1080, 960, 540);

        // Map a point to inference space and back
        let raw_point = (960.0, 540.0); // Center of 1080p
        let (inf_x, inf_y) = meta.map_point_to_inf(raw_point.0, raw_point.1);
        let (back_x, back_y) = meta.map_point(inf_x, inf_y);

        assert!((back_x - raw_point.0).abs() < 1.0);
        assert!((back_y - raw_point.1).abs() < 1.0);
    }

    #[test]
    fn test_zero_bar_output() {
        // Detections should never include padding coordinates
        let meta = MappingMeta::compute(1280, 720, 960, 540);

        // Create a detection at the edge of the active area
        // With pillarboxing, active area starts at x=120
        let bbox_inf = BoundingBox::new(120.0, 0.0, 100.0, 100.0);
        let bbox_raw = meta.map_rect(&bbox_inf);

        // Should map to x=0 in raw space (edge of frame)
        assert!(bbox_raw.x >= 0.0);
        assert!(bbox_raw.x < 1.0); // Should be at or near edge
    }

    #[test]
    fn test_yunet_padding_value() {
        let meta = MappingMeta::for_yunet(1920, 1080, 960, 540);
        assert_eq!(meta.padding_value, 0); // YuNet expects black
    }

    #[test]
    fn test_youtube_optimized_sizes() {
        // 1080p should use 960x540
        let meta = MappingMeta::for_youtube(1920, 1080);
        assert_eq!(meta.inf_width, 960);
        assert_eq!(meta.inf_height, 540);

        // 720p should use 640x360
        let meta = MappingMeta::for_youtube(1280, 720);
        assert_eq!(meta.inf_width, 640);
        assert_eq!(meta.inf_height, 360);
    }

    #[test]
    fn test_normalized_bbox() {
        let meta = MappingMeta::compute(1920, 1080, 960, 540);
        let bbox = BoundingBox::new(480.0, 270.0, 960.0, 540.0);

        let norm = meta.normalize(&bbox);
        assert!((norm.x - 0.25).abs() < 1e-10);
        assert!((norm.y - 0.25).abs() < 1e-10);
        assert!((norm.w - 0.5).abs() < 1e-10);
        assert!((norm.h - 0.5).abs() < 1e-10);

        // Denormalize back
        let denorm = meta.denormalize(&norm);
        assert!((denorm.x - bbox.x).abs() < 1e-10);
        assert!((denorm.y - bbox.y).abs() < 1e-10);
    }

    #[test]
    fn test_iou_calculation() {
        let bbox1 = NormalizedBBox::new(0.0, 0.0, 0.5, 0.5);
        let bbox2 = NormalizedBBox::new(0.25, 0.25, 0.5, 0.5);

        let iou = bbox1.iou(&bbox2);
        // Overlap: 0.25 * 0.25 = 0.0625
        // Union: 0.25 + 0.25 - 0.0625 = 0.4375
        // IoU: 0.0625 / 0.4375 ≈ 0.143
        assert!((iou - 0.14285714).abs() < 0.01);
    }

    #[test]
    fn test_is_in_active_area() {
        let meta = MappingMeta::compute(640, 480, 960, 540);
        // With pillarboxing, active area is 720x540 starting at (120, 0)

        assert!(meta.is_in_active_area(500.0, 270.0)); // Center, should be active
        assert!(!meta.is_in_active_area(50.0, 270.0)); // In left padding
        assert!(!meta.is_in_active_area(900.0, 270.0)); // In right padding
    }

    #[test]
    fn test_9x16_vertical_source() {
        // 1080x1920 (9:16 vertical) -> 960x540 needs significant letterboxing
        let meta = MappingMeta::compute(1080, 1920, 960, 540);

        // Scale limited by height: 540/1920 = 0.28125
        // Scaled width: 1080 * 0.28125 = 303.75 ≈ 304
        assert!((meta.scale - 0.28125).abs() < 1e-5);
        assert!(meta.scaled_width < meta.scaled_height || meta.scaled_width < 400);
        assert!(meta.pad_left > 200); // Significant pillarboxing
    }

    #[test]
    fn test_21x9_ultrawide() {
        // 2560x1080 (21:9) -> 960x540 needs letterboxing
        let meta = MappingMeta::compute(2560, 1080, 960, 540);

        // Scale limited by width: 960/2560 = 0.375
        // Scaled height: 1080 * 0.375 = 405
        assert!((meta.scale - 0.375).abs() < 1e-5);
        assert_eq!(meta.scaled_width, 960);
        assert!(meta.scaled_height < 540);
        assert!(meta.pad_top > 0); // Top/bottom letterboxing
    }
}
