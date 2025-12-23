//! AVFrame Safety Wrapper with Rust Lifetimes
//!
//! Provides safe, zero-copy wrappers for FFmpeg AVFrame data with proper
//! lifetime guarantees. Enables passing decoded frames to inference
//! without unnecessary copies.
//!
//! # Safety Model
//! - `AvFrameView` borrows data from an `AVFrame` with explicit lifetime
//! - `SharedAvFrame` provides thread-safe reference counting
//! - All operations that access raw pointers are unsafe and documented
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::avframe_view::AvFrameView;
//!
//! // From decoder
//! let frame = decoder.decode_frame()?;
//!
//! // Create safe view (borrows frame)
//! let view = AvFrameView::from_frame(&frame)?;
//!
//! // Use view for inference
//! let detections = detector.detect(&view)?;
//!
//! // Frame automatically valid until view is dropped
//! ```

use std::marker::PhantomData;
use std::sync::Arc;

/// Pixel format enumeration for AVFrame data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// YUV 4:2:0 planar (3 planes: Y, U, V)
    Yuv420p,
    /// NV12 (2 planes: Y, interleaved UV)
    Nv12,
    /// BGR24 packed (1 plane)
    Bgr24,
    /// RGB24 packed (1 plane)
    Rgb24,
    /// BGRA32 packed (1 plane)
    Bgra32,
    /// Unknown format
    Unknown(i32),
}

impl PixelFormat {
    /// Number of planes for this format.
    pub fn num_planes(&self) -> usize {
        match self {
            PixelFormat::Yuv420p => 3,
            PixelFormat::Nv12 => 2,
            PixelFormat::Bgr24 | PixelFormat::Rgb24 => 1,
            PixelFormat::Bgra32 => 1,
            PixelFormat::Unknown(_) => 0,
        }
    }

    /// Bytes per pixel for packed formats.
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            PixelFormat::Bgr24 | PixelFormat::Rgb24 => Some(3),
            PixelFormat::Bgra32 => Some(4),
            _ => None, // Planar formats don't have fixed BPP
        }
    }

    /// Check if format is planar (YUV).
    pub fn is_planar(&self) -> bool {
        matches!(self, PixelFormat::Yuv420p | PixelFormat::Nv12)
    }
}

/// Error type for frame operations.
#[derive(Debug, Clone)]
pub enum FrameError {
    /// Frame has null data pointer
    NullData,
    /// Unsupported pixel format
    UnsupportedFormat(i32),
    /// Invalid dimensions
    InvalidDimensions { width: i32, height: i32 },
    /// Plane index out of bounds
    PlaneIndexOutOfBounds { index: usize, max: usize },
    /// Frame data not contiguous
    NonContiguous,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::NullData => write!(f, "Frame has null data pointer"),
            FrameError::UnsupportedFormat(fmt) => {
                write!(f, "Unsupported pixel format: {}", fmt)
            }
            FrameError::InvalidDimensions { width, height } => {
                write!(f, "Invalid dimensions: {}x{}", width, height)
            }
            FrameError::PlaneIndexOutOfBounds { index, max } => {
                write!(f, "Plane index {} out of bounds (max: {})", index, max)
            }
            FrameError::NonContiguous => write!(f, "Frame data is not contiguous"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Immutable view into a single plane of an AVFrame.
///
/// Provides safe, bounds-checked access to plane data.
#[derive(Debug)]
pub struct PlaneView<'a> {
    /// Pointer to plane data
    data: *const u8,
    /// Width of plane in pixels
    width: i32,
    /// Height of plane in pixels
    height: i32,
    /// Stride (bytes per row, may include padding)
    stride: i32,
    /// Lifetime marker
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> PlaneView<'a> {
    /// Create a new plane view.
    ///
    /// # Safety
    /// Caller must ensure data pointer is valid for the specified dimensions
    /// and lifetime 'a.
    pub unsafe fn new(data: *const u8, width: i32, height: i32, stride: i32) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            _marker: PhantomData,
        }
    }

    /// Get plane width in pixels.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Get plane height in pixels.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Get plane stride (bytes per row).
    pub fn stride(&self) -> i32 {
        self.stride
    }

    /// Get raw data pointer.
    ///
    /// # Safety
    /// Caller must ensure pointer is not used beyond the lifetime 'a.
    pub unsafe fn as_ptr(&self) -> *const u8 {
        self.data
    }

    /// Get pointer to specific row.
    ///
    /// # Safety
    /// Caller must ensure row index is valid and pointer is not used
    /// beyond lifetime 'a.
    pub unsafe fn row_ptr(&self, row: i32) -> Option<*const u8> {
        if row < 0 || row >= self.height {
            None
        } else {
            Some(self.data.add((row * self.stride) as usize))
        }
    }

    /// Get byte at specific position.
    ///
    /// Returns None if position is out of bounds.
    pub fn get(&self, row: i32, col: i32) -> Option<u8> {
        if row < 0 || row >= self.height || col < 0 || col >= self.stride {
            None
        } else {
            unsafe {
                let offset = (row * self.stride + col) as usize;
                Some(*self.data.add(offset))
            }
        }
    }

    /// Get row as slice.
    ///
    /// Returns None if row is out of bounds.
    pub fn row_slice(&self, row: i32) -> Option<&'a [u8]> {
        if row < 0 || row >= self.height {
            None
        } else {
            unsafe {
                let offset = (row * self.stride) as usize;
                Some(std::slice::from_raw_parts(
                    self.data.add(offset),
                    self.stride as usize,
                ))
            }
        }
    }

    /// Total size of plane data in bytes.
    pub fn size_bytes(&self) -> usize {
        (self.height * self.stride) as usize
    }
}

/// Safe view into AVFrame data with lifetime tracking.
///
/// This struct borrows data from an AVFrame and provides safe access
/// to pixel data through the Rust lifetime system.
pub struct AvFrameView<'a> {
    /// Pixel format
    format: PixelFormat,
    /// Frame width
    width: i32,
    /// Frame height
    height: i32,
    /// Plane data pointers
    planes: [Option<PlaneView<'a>>; 4],
    /// Number of valid planes
    num_planes: usize,
    /// Phantom data for lifetime
    _marker: PhantomData<&'a ()>,
}

impl<'a> AvFrameView<'a> {
    /// Create a view from raw frame data.
    ///
    /// # Safety
    /// Caller must ensure:
    /// - All data pointers are valid for the frame dimensions
    /// - Data remains valid for lifetime 'a
    /// - Strides are correct for the pixel format
    pub unsafe fn from_raw(
        format: PixelFormat,
        width: i32,
        height: i32,
        data_ptrs: &[*const u8],
        strides: &[i32],
    ) -> Result<Self, FrameError> {
        if width <= 0 || height <= 0 {
            return Err(FrameError::InvalidDimensions { width, height });
        }

        let num_planes = format.num_planes();
        if data_ptrs.len() < num_planes || strides.len() < num_planes {
            return Err(FrameError::NullData);
        }

        let mut planes: [Option<PlaneView<'a>>; 4] = [None, None, None, None];

        for i in 0..num_planes {
            if data_ptrs[i].is_null() {
                return Err(FrameError::NullData);
            }

            let (plane_w, plane_h) = match format {
                PixelFormat::Yuv420p if i > 0 => (width / 2, height / 2),
                PixelFormat::Nv12 if i == 1 => (width, height / 2),
                _ => (width, height),
            };

            planes[i] = Some(PlaneView::new(data_ptrs[i], plane_w, plane_h, strides[i]));
        }

        Ok(Self {
            format,
            width,
            height,
            planes,
            num_planes,
            _marker: PhantomData,
        })
    }

    /// Get pixel format.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Get frame width.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Get frame height.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Get number of planes.
    pub fn num_planes(&self) -> usize {
        self.num_planes
    }

    /// Get plane by index.
    pub fn plane(&self, index: usize) -> Result<&PlaneView<'a>, FrameError> {
        if index >= self.num_planes {
            return Err(FrameError::PlaneIndexOutOfBounds {
                index,
                max: self.num_planes,
            });
        }

        self.planes[index]
            .as_ref()
            .ok_or(FrameError::PlaneIndexOutOfBounds {
                index,
                max: self.num_planes,
            })
    }

    /// Get Y plane (for YUV formats).
    pub fn y_plane(&self) -> Result<&PlaneView<'a>, FrameError> {
        if !self.format.is_planar() {
            return Err(FrameError::UnsupportedFormat(0));
        }
        self.plane(0)
    }

    /// Check if frame is YUV format.
    pub fn is_yuv(&self) -> bool {
        self.format.is_planar()
    }

    /// Check if frame is packed RGB/BGR.
    pub fn is_packed_rgb(&self) -> bool {
        matches!(
            self.format,
            PixelFormat::Bgr24 | PixelFormat::Rgb24 | PixelFormat::Bgra32
        )
    }
}

/// Thread-safe shared AVFrame with reference counting.
///
/// Allows passing frame data across thread boundaries safely.
pub struct SharedAvFrame {
    /// Reference-counted frame data
    inner: Arc<SharedFrameInner>,
}

struct SharedFrameInner {
    /// Owned pixel data
    data: Vec<u8>,
    /// Format
    format: PixelFormat,
    /// Dimensions
    width: i32,
    height: i32,
    /// Stride
    stride: i32,
}

impl SharedAvFrame {
    /// Create a new shared frame by copying pixel data.
    ///
    /// This makes a copy of the data to enable sharing across threads.
    pub fn from_bgr_data(data: Vec<u8>, width: i32, height: i32, stride: i32) -> Self {
        Self {
            inner: Arc::new(SharedFrameInner {
                data,
                format: PixelFormat::Bgr24,
                width,
                height,
                stride,
            }),
        }
    }

    /// Get frame dimensions.
    pub fn dimensions(&self) -> (i32, i32) {
        (self.inner.width, self.inner.height)
    }

    /// Get pixel format.
    pub fn format(&self) -> PixelFormat {
        self.inner.format
    }

    /// Get data as slice.
    pub fn data(&self) -> &[u8] {
        &self.inner.data
    }

    /// Get stride.
    pub fn stride(&self) -> i32 {
        self.inner.stride
    }

    /// Clone the Arc (cheap reference count increment).
    pub fn share(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Get reference count.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl Clone for SharedAvFrame {
    fn clone(&self) -> Self {
        self.share()
    }
}

// SharedAvFrame is Send + Sync because Arc<T> is Send + Sync when T is
unsafe impl Send for SharedAvFrame {}
unsafe impl Sync for SharedAvFrame {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_planes() {
        assert_eq!(PixelFormat::Yuv420p.num_planes(), 3);
        assert_eq!(PixelFormat::Nv12.num_planes(), 2);
        assert_eq!(PixelFormat::Bgr24.num_planes(), 1);
    }

    #[test]
    fn test_pixel_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::Bgr24.bytes_per_pixel(), Some(3));
        assert_eq!(PixelFormat::Bgra32.bytes_per_pixel(), Some(4));
        assert_eq!(PixelFormat::Yuv420p.bytes_per_pixel(), None);
    }

    #[test]
    fn test_plane_view() {
        let data = vec![0u8; 640 * 480];
        let view = unsafe { PlaneView::new(data.as_ptr(), 640, 480, 640) };

        assert_eq!(view.width(), 640);
        assert_eq!(view.height(), 480);
        assert_eq!(view.stride(), 640);
        assert_eq!(view.size_bytes(), 640 * 480);
    }

    #[test]
    fn test_plane_view_bounds() {
        let data = vec![42u8; 10 * 10];
        let view = unsafe { PlaneView::new(data.as_ptr(), 10, 10, 10) };

        assert_eq!(view.get(0, 0), Some(42));
        assert_eq!(view.get(5, 5), Some(42));
        assert_eq!(view.get(10, 0), None); // Out of bounds
        assert_eq!(view.get(0, 10), None); // Out of bounds
    }

    #[test]
    fn test_shared_frame() {
        let data = vec![128u8; 640 * 480 * 3];
        let frame = SharedAvFrame::from_bgr_data(data, 640, 480, 640 * 3);

        assert_eq!(frame.dimensions(), (640, 480));
        assert_eq!(frame.format(), PixelFormat::Bgr24);
        assert_eq!(frame.ref_count(), 1);

        let shared = frame.share();
        assert_eq!(frame.ref_count(), 2);
        assert_eq!(shared.ref_count(), 2);
    }

    #[test]
    fn test_shared_frame_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<SharedAvFrame>();
        assert_sync::<SharedAvFrame>();
    }

    #[test]
    fn test_avframe_view_creation() {
        let y_data = vec![0u8; 640 * 480];
        let u_data = vec![128u8; 320 * 240];
        let v_data = vec![128u8; 320 * 240];

        let data_ptrs = [
            y_data.as_ptr(),
            u_data.as_ptr(),
            v_data.as_ptr(),
            std::ptr::null(),
        ];
        let strides = [640, 320, 320, 0];

        let view = unsafe {
            AvFrameView::from_raw(PixelFormat::Yuv420p, 640, 480, &data_ptrs, &strides)
        };

        assert!(view.is_ok());
        let view = view.unwrap();
        assert_eq!(view.width(), 640);
        assert_eq!(view.height(), 480);
        assert_eq!(view.num_planes(), 3);
        assert!(view.is_yuv());
    }
}
