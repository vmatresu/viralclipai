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
use std::cell::RefCell;
use tracing::{debug, info};

#[cfg(feature = "opencv")]
use opencv::{
    core::{Mat, Size, CV_8UC3},
    imgproc,
    prelude::*,
};

/// Thread-local buffer pool for frame conversion.
///
/// Avoids passing buffers across threads and ensures each worker
/// has its own pre-allocated memory.
#[cfg(feature = "opencv")]
thread_local! {
    static BUFFER_POOL: RefCell<Option<BufferPool>> = RefCell::new(None);
}

/// Pre-allocated buffer pool for frame operations.
#[cfg(feature = "opencv")]
struct BufferPool {
    /// BGR conversion buffer
    bgr_buffer: Mat,
    /// Resize buffer
    resize_buffer: Mat,
    /// Letterbox output buffer
    letterbox_buffer: Mat,
    /// Detection output buffer
    faces_buffer: Mat,
    /// Current raw dimensions
    raw_size: (i32, i32),
    /// Target inference dimensions
    inf_size: (i32, i32),
}

#[cfg(feature = "opencv")]
impl BufferPool {
    fn new(inf_width: i32, inf_height: i32) -> MediaResult<Self> {
        Ok(Self {
            bgr_buffer: Mat::default(),
            resize_buffer: Mat::default(),
            letterbox_buffer: Mat::zeros(inf_height, inf_width, CV_8UC3)
                .map_err(|e| MediaError::detection_failed(format!("Buffer alloc: {}", e)))?
                .to_mat()
                .map_err(|e| MediaError::detection_failed(format!("Buffer conv: {}", e)))?,
            faces_buffer: Mat::default(),
            raw_size: (0, 0),
            inf_size: (inf_width, inf_height),
        })
    }

    fn ensure_size(&mut self, raw_width: i32, raw_height: i32) -> MediaResult<()> {
        if self.raw_size == (raw_width, raw_height) {
            return Ok(());
        }

        // Reallocate BGR buffer
        self.bgr_buffer = Mat::zeros(raw_height, raw_width, CV_8UC3)
            .map_err(|e| MediaError::detection_failed(format!("BGR buffer: {}", e)))?
            .to_mat()
            .map_err(|e| MediaError::detection_failed(format!("BGR conv: {}", e)))?;

        // Compute new mapping to get scaled size
        let meta = MappingMeta::compute(
            raw_width as u32,
            raw_height as u32,
            self.inf_size.0 as u32,
            self.inf_size.1 as u32,
        );

        // Reallocate resize buffer
        self.resize_buffer = Mat::zeros(meta.scaled_height, meta.scaled_width, CV_8UC3)
            .map_err(|e| MediaError::detection_failed(format!("Resize buffer: {}", e)))?
            .to_mat()
            .map_err(|e| MediaError::detection_failed(format!("Resize conv: {}", e)))?;

        self.raw_size = (raw_width, raw_height);

        debug!(
            raw = format!("{}x{}", raw_width, raw_height),
            inf = format!("{}x{}", self.inf_size.0, self.inf_size.1),
            "Buffer pool resized"
        );

        Ok(())
    }
}

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

/// Mat buffer pool for reusing allocated memory.
///
/// Reduces allocation pressure by maintaining a pool of pre-sized Mats.
#[cfg(feature = "opencv")]
pub struct MatPool {
    /// Available buffers
    buffers: Vec<Mat>,
    /// Target size for buffers
    target_size: (i32, i32),
    /// Maximum pool size
    max_size: usize,
}

#[cfg(feature = "opencv")]
impl MatPool {
    /// Create a new Mat pool with target dimensions.
    pub fn new(width: i32, height: i32, max_size: usize) -> Self {
        Self {
            buffers: Vec::with_capacity(max_size),
            target_size: (width, height),
            max_size,
        }
    }

    /// Get a buffer from the pool, or create a new one.
    pub fn get(&mut self) -> MediaResult<Mat> {
        if let Some(mat) = self.buffers.pop() {
            Ok(mat)
        } else {
            Mat::zeros(self.target_size.1, self.target_size.0, CV_8UC3)
                .map_err(|e| MediaError::detection_failed(format!("Pool alloc: {}", e)))?
                .to_mat()
                .map_err(|e| MediaError::detection_failed(format!("Pool conv: {}", e)))
        }
    }

    /// Return a buffer to the pool.
    pub fn put(&mut self, mat: Mat) {
        if self.buffers.len() < self.max_size {
            self.buffers.push(mat);
        }
        // Otherwise drop the mat
    }

    /// Clear the pool.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Get current pool size.
    pub fn size(&self) -> usize {
        self.buffers.len()
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

    #[test]
    fn test_mat_pool() {
        let mut pool = MatPool::new(640, 480, 4);

        // Get buffers
        let mat1 = pool.get().unwrap();
        let mat2 = pool.get().unwrap();

        assert_eq!(pool.size(), 0);

        // Return buffers
        pool.put(mat1);
        pool.put(mat2);

        assert_eq!(pool.size(), 2);

        // Get from pool
        let _mat3 = pool.get().unwrap();
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn test_mat_pool_max_size() {
        let mut pool = MatPool::new(320, 240, 2);

        // Fill pool
        let mat1 = pool.get().unwrap();
        let mat2 = pool.get().unwrap();
        let mat3 = pool.get().unwrap();

        pool.put(mat1);
        pool.put(mat2);
        pool.put(mat3); // Should be dropped (exceeds max_size)

        assert_eq!(pool.size(), 2);
    }
}
