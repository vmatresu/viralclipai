//! Fixed-Resolution Letterbox Preprocessing
//!
//! Provides efficient letterboxing operations for preparing frames
//! for neural network inference with consistent dimensions.
//!
//! # Features
//! - Fixed canvas size (e.g., 960x540) for stable performance
//! - Aspect-preserving scaling with INTER_AREA for downscaling
//! - Centered padding with model-appropriate fill value
//! - Pre-allocated buffer support for zero hot-loop allocations
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::letterbox::Letterboxer;
//!
//! let mut letterboxer = Letterboxer::new(960, 540);
//! let (letterboxed, meta) = letterboxer.process(&frame)?;
//! ```

use super::mapping::MappingMeta;
use crate::error::{MediaError, MediaResult};
use tracing::debug;

#[cfg(feature = "opencv")]
use opencv::{
    core::{Mat, Scalar, Size, BORDER_CONSTANT},
    imgproc,
    prelude::*,
};

/// Letterboxer for fixed-resolution inference preprocessing.
///
/// Maintains pre-allocated buffers for zero-allocation steady-state operation.
#[cfg(feature = "opencv")]
pub struct Letterboxer {
    /// Target inference canvas width
    inf_width: i32,
    /// Target inference canvas height
    inf_height: i32,
    /// Padding fill value (0-255)
    padding_value: u8,
    /// Pre-allocated resize buffer
    resize_buffer: Mat,
    /// Pre-allocated letterbox output buffer
    output_buffer: Mat,
    /// Current raw frame dimensions (for buffer reuse)
    current_raw_size: Option<(i32, i32)>,
}

#[cfg(feature = "opencv")]
impl Letterboxer {
    /// Create a new letterboxer with specified inference dimensions.
    ///
    /// # Arguments
    /// * `inf_width` - Target canvas width (should be multiple of 32)
    /// * `inf_height` - Target canvas height (should be multiple of 32)
    pub fn new(inf_width: i32, inf_height: i32) -> Self {
        Self {
            inf_width,
            inf_height,
            padding_value: 0, // YuNet default
            resize_buffer: Mat::default(),
            output_buffer: Mat::default(),
            current_raw_size: None,
        }
    }

    /// Create with YuNet-optimized settings.
    pub fn for_yunet() -> Self {
        Self::new(960, 540)
    }

    /// Set the padding fill value.
    ///
    /// # Arguments
    /// * `value` - Fill value (0 = black, 128 = gray)
    pub fn with_padding_value(mut self, value: u8) -> Self {
        self.padding_value = value;
        self
    }

    /// Process a frame with letterboxing.
    ///
    /// Returns the letterboxed frame and mapping metadata.
    /// Uses pre-allocated buffers for efficiency.
    pub fn process(&mut self, frame: &Mat) -> MediaResult<(&Mat, MappingMeta)> {
        if frame.empty() {
            return Err(MediaError::detection_failed("Empty frame provided"));
        }

        let raw_width = frame.cols();
        let raw_height = frame.rows();

        // Compute mapping
        let meta = MappingMeta::for_yunet(
            raw_width as u32,
            raw_height as u32,
            self.inf_width as u32,
            self.inf_height as u32,
        );

        // Check if we need to reallocate output buffer
        if self.current_raw_size != Some((raw_width, raw_height)) {
            self.allocate_buffers(raw_width, raw_height, &meta)?;
            self.current_raw_size = Some((raw_width, raw_height));
        }

        // Step 1: Resize with aspect preservation
        let scaled_size = Size::new(meta.scaled_width, meta.scaled_height);
        imgproc::resize(
            frame,
            &mut self.resize_buffer,
            scaled_size,
            0.0,
            0.0,
            imgproc::INTER_AREA, // INTER_AREA is optimal for downscaling
        )
        .map_err(|e| MediaError::detection_failed(format!("Resize failed: {}", e)))?;

        // Step 2: Apply letterbox padding
        let (pad_left, pad_top, pad_right, pad_bottom) = meta.padding();
        let pad_color = Scalar::all(self.padding_value as f64);

        opencv::core::copy_make_border(
            &self.resize_buffer,
            &mut self.output_buffer,
            pad_top,
            pad_bottom,
            pad_left,
            pad_right,
            BORDER_CONSTANT,
            pad_color,
        )
        .map_err(|e| MediaError::detection_failed(format!("Padding failed: {}", e)))?;

        debug!(
            raw = format!("{}x{}", raw_width, raw_height),
            scaled = format!("{}x{}", meta.scaled_width, meta.scaled_height),
            padding = format!(
                "l={} t={} r={} b={}",
                pad_left, pad_top, pad_right, pad_bottom
            ),
            "Letterbox complete"
        );

        Ok((&self.output_buffer, meta))
    }

    /// Allocate/reallocate buffers for new frame dimensions.
    fn allocate_buffers(
        &mut self,
        raw_width: i32,
        raw_height: i32,
        meta: &MappingMeta,
    ) -> MediaResult<()> {
        // Pre-allocate resize buffer
        self.resize_buffer =
            Mat::zeros(meta.scaled_height, meta.scaled_width, opencv::core::CV_8UC3)
                .map_err(|e| MediaError::detection_failed(format!("Buffer alloc failed: {}", e)))?
                .to_mat()
                .map_err(|e| {
                    MediaError::detection_failed(format!("Buffer conversion failed: {}", e))
                })?;

        // Pre-allocate output buffer
        self.output_buffer = Mat::zeros(self.inf_height, self.inf_width, opencv::core::CV_8UC3)
            .map_err(|e| MediaError::detection_failed(format!("Buffer alloc failed: {}", e)))?
            .to_mat()
            .map_err(|e| {
                MediaError::detection_failed(format!("Buffer conversion failed: {}", e))
            })?;

        debug!(
            raw = format!("{}x{}", raw_width, raw_height),
            inf = format!("{}x{}", self.inf_width, self.inf_height),
            "Allocated letterbox buffers"
        );

        Ok(())
    }

    /// Get the current inference canvas size.
    pub fn canvas_size(&self) -> (i32, i32) {
        (self.inf_width, self.inf_height)
    }

    /// Get the padding value.
    pub fn padding_value(&self) -> u8 {
        self.padding_value
    }
}

/// Convenience function for one-shot letterboxing.
///
/// Creates a temporary Letterboxer and processes a single frame.
/// For batch processing, use `Letterboxer` directly to reuse buffers.
#[cfg(feature = "opencv")]
pub fn letterbox_frame(
    frame: &Mat,
    inf_width: i32,
    inf_height: i32,
) -> MediaResult<(Mat, MappingMeta)> {
    let mut letterboxer = Letterboxer::new(inf_width, inf_height);
    let (result, meta) = letterboxer.process(frame)?;
    Ok((result.clone(), meta))
}

/// Calculate optimal inference size for given raw dimensions.
///
/// Selects dimensions that:
/// 1. Are multiples of 32 (CNN alignment)
/// 2. Minimize padding for the given aspect ratio
/// 3. Stay within reasonable bounds (160-960 width)
pub fn optimal_inference_size(raw_width: u32, raw_height: u32) -> (i32, i32) {
    const ALIGNMENT: i32 = 32;
    const MIN_DIM: i32 = 160;
    const MAX_WIDTH: i32 = 960;
    const MAX_HEIGHT: i32 = 540;

    // Target ~0.5M pixels (similar to 960x540)
    const TARGET_PIXELS: f64 = 518400.0;

    let aspect = raw_width as f64 / raw_height as f64;

    // Calculate dimensions maintaining aspect ratio
    let height = (TARGET_PIXELS / aspect).sqrt();
    let width = height * aspect;

    // Round to alignment
    let mut inf_width = ((width as i32 + ALIGNMENT / 2) / ALIGNMENT) * ALIGNMENT;
    let mut inf_height = ((height as i32 + ALIGNMENT / 2) / ALIGNMENT) * ALIGNMENT;

    // Clamp to bounds
    inf_width = inf_width.clamp(MIN_DIM, MAX_WIDTH);
    inf_height = inf_height.clamp(MIN_DIM, MAX_HEIGHT);

    (inf_width, inf_height)
}

#[cfg(feature = "opencv")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letterboxer_creation() {
        let letterboxer = Letterboxer::new(960, 540);
        assert_eq!(letterboxer.canvas_size(), (960, 540));
        assert_eq!(letterboxer.padding_value(), 0);
    }

    #[test]
    fn test_letterboxer_with_padding() {
        let letterboxer = Letterboxer::new(960, 540).with_padding_value(128);
        assert_eq!(letterboxer.padding_value(), 128);
    }

    #[test]
    fn test_optimal_size_16x9() {
        let (w, h) = optimal_inference_size(1920, 1080);
        assert!(w % 32 == 0);
        assert!(h % 32 == 0);
        assert!(w <= 960);
        assert!(h <= 540);
    }

    #[test]
    fn test_optimal_size_4x3() {
        let (w, h) = optimal_inference_size(640, 480);
        assert!(w % 32 == 0);
        assert!(h % 32 == 0);
    }

    #[test]
    fn test_optimal_size_vertical() {
        let (w, h) = optimal_inference_size(1080, 1920);
        assert!(w % 32 == 0);
        assert!(h % 32 == 0);
        // Vertical video should have narrower width
        assert!(w < h || w < 500);
    }
}

/// Non-OpenCV stub for when opencv feature is disabled.
#[cfg(not(feature = "opencv"))]
pub struct Letterboxer {
    inf_width: i32,
    inf_height: i32,
    padding_value: u8,
}

#[cfg(not(feature = "opencv"))]
impl Letterboxer {
    pub fn new(inf_width: i32, inf_height: i32) -> Self {
        Self {
            inf_width,
            inf_height,
            padding_value: 0,
        }
    }

    pub fn for_yunet() -> Self {
        Self::new(960, 540)
    }

    pub fn with_padding_value(mut self, value: u8) -> Self {
        self.padding_value = value;
        self
    }

    pub fn canvas_size(&self) -> (i32, i32) {
        (self.inf_width, self.inf_height)
    }

    pub fn padding_value(&self) -> u8 {
        self.padding_value
    }
}
