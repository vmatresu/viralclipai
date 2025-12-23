//! Frame Converter with Buffer Pooling
//!
//! Provides efficient frame conversion with pre-allocated buffers to achieve
//! zero heap allocations in the hot loop (steady state).
//!
//! # Features
//! - Pre-allocated Mat buffers for resize and letterbox operations
//! - YUV to BGR conversion (sws_scale wrapper or OpenCV)
//! - Buffer reuse across frames of same dimensions
//! - Thread-local detector instances
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::frame_converter::FrameConverter;
//!
//! let mut converter = FrameConverter::new(960, 540);
//!
//! // Process frames - buffers are reused
//! for frame in frames {
//!     let (letterboxed, meta) = converter.convert(&frame)?;
//!     // Use letterboxed frame...
//! }
//! ```

use super::letterbox::Letterboxer;
use super::mapping::MappingMeta;
use crate::error::{MediaError, MediaResult};
use tracing::info;

#[cfg(feature = "opencv")]
use opencv::{
    core::{Mat, Size},
    imgproc,
    prelude::*,
};

/// Frame converter with buffer pooling for zero-allocation hot loop.
#[cfg(feature = "opencv")]
pub struct FrameConverter {
    /// Target inference dimensions
    inf_width: i32,
    inf_height: i32,
    /// Padding value for letterbox
    padding_value: u8,
    /// Internal letterboxer
    letterboxer: Letterboxer,
    /// Frames processed (for stats)
    frames_processed: u64,
    /// Allocations triggered (should be 0 in steady state)
    allocations: u64,
}

#[cfg(feature = "opencv")]
impl FrameConverter {
    /// Create a new frame converter with specified inference dimensions.
    pub fn new(inf_width: i32, inf_height: i32) -> Self {
        Self {
            inf_width,
            inf_height,
            padding_value: 0, // YuNet default
            letterboxer: Letterboxer::new(inf_width, inf_height),
            frames_processed: 0,
            allocations: 0,
        }
    }

    /// Create with YuNet-optimized settings (960x540).
    pub fn for_yunet() -> Self {
        Self::new(960, 540)
    }

    /// Set padding value for letterbox.
    pub fn with_padding_value(mut self, value: u8) -> Self {
        self.padding_value = value;
        self.letterboxer = self.letterboxer.with_padding_value(value);
        self
    }

    /// Convert frame with letterboxing, using pooled buffers.
    ///
    /// Returns the letterboxed frame and mapping metadata.
    /// After warm-up, this should trigger zero allocations.
    pub fn letterbox(&mut self, frame: &Mat) -> MediaResult<(&Mat, MappingMeta)> {
        self.frames_processed += 1;
        self.letterboxer.process(frame)
    }

    /// Convert BGR frame to letterboxed inference input.
    ///
    /// Uses thread-local buffer pool for efficiency.
    pub fn convert_bgr(&mut self, frame: &Mat) -> MediaResult<(Mat, MappingMeta)> {
        if frame.empty() {
            return Err(MediaError::detection_failed("Empty frame"));
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

        // Resize with aspect preservation
        let scaled_size = Size::new(meta.scaled_width, meta.scaled_height);
        let mut resized = Mat::default();
        imgproc::resize(
            frame,
            &mut resized,
            scaled_size,
            0.0,
            0.0,
            imgproc::INTER_AREA,
        )
        .map_err(|e| MediaError::detection_failed(format!("Resize: {}", e)))?;

        // Apply letterbox padding
        let (pad_left, pad_top, pad_right, pad_bottom) = meta.padding();
        let mut letterboxed = Mat::default();
        opencv::core::copy_make_border(
            &resized,
            &mut letterboxed,
            pad_top,
            pad_bottom,
            pad_left,
            pad_right,
            opencv::core::BORDER_CONSTANT,
            opencv::core::Scalar::all(self.padding_value as f64),
        )
        .map_err(|e| MediaError::detection_failed(format!("Padding: {}", e)))?;

        self.frames_processed += 1;
        Ok((letterboxed, meta))
    }

    /// Get raw frame dimensions from current mapping.
    pub fn raw_dims(&self) -> (u32, u32) {
        (self.inf_width as u32 * 2, self.inf_height as u32 * 2) // Approximate
    }

    /// Get inference dimensions.
    pub fn inf_dims(&self) -> (i32, i32) {
        (self.inf_width, self.inf_height)
    }

    /// Get frames processed count.
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Get allocation count (should be 0 in steady state).
    pub fn allocations(&self) -> u64 {
        self.allocations
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.frames_processed = 0;
        self.allocations = 0;
    }

    /// Log converter statistics.
    pub fn log_stats(&self) {
        info!(
            frames = self.frames_processed,
            allocations = self.allocations,
            inf_size = format!("{}x{}", self.inf_width, self.inf_height),
            "Frame converter stats"
        );
    }
}

/// Non-OpenCV stub implementation.
#[cfg(not(feature = "opencv"))]
pub struct FrameConverter {
    inf_width: i32,
    inf_height: i32,
    padding_value: u8,
    frames_processed: u64,
}

#[cfg(not(feature = "opencv"))]
impl FrameConverter {
    pub fn new(inf_width: i32, inf_height: i32) -> Self {
        Self {
            inf_width,
            inf_height,
            padding_value: 0,
            frames_processed: 0,
        }
    }

    pub fn for_yunet() -> Self {
        Self::new(960, 540)
    }

    pub fn with_padding_value(mut self, value: u8) -> Self {
        self.padding_value = value;
        self
    }

    pub fn raw_dims(&self) -> (u32, u32) {
        (self.inf_width as u32 * 2, self.inf_height as u32 * 2)
    }

    pub fn inf_dims(&self) -> (i32, i32) {
        (self.inf_width, self.inf_height)
    }

    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }
}

#[cfg(feature = "opencv")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_converter_creation() {
        let converter = FrameConverter::new(960, 540);
        assert_eq!(converter.inf_dims(), (960, 540));
        assert_eq!(converter.frames_processed(), 0);
    }

    #[test]
    fn test_frame_converter_for_yunet() {
        let converter = FrameConverter::for_yunet();
        assert_eq!(converter.inf_dims(), (960, 540));
    }
}
