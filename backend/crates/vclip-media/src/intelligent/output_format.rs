//! Output format constants for portrait video rendering.
//!
//! This module provides a single source of truth for target output dimensions.
//! All renderers should use these constants to ensure consistent 9:16 portrait
//! output at exactly 1080×1920 pixels.
//!
//! # FFmpeg Constraints
//!
//! - libx264 requires width/height to be divisible by 2
//! - SAR (Sample Aspect Ratio) must be 1:1 for square pixels
//! - DAR (Display Aspect Ratio) should be 9:16

/// Target width for portrait (9:16) output.
pub const PORTRAIT_WIDTH: u32 = 1080;

/// Target height for portrait (9:16) output.
pub const PORTRAIT_HEIGHT: u32 = 1920;

/// Panel width for split-view (half of portrait height).
pub const SPLIT_PANEL_WIDTH: u32 = 1080;

/// Panel height for split-view (half of portrait height).
pub const SPLIT_PANEL_HEIGHT: u32 = 960;

/// Builds an FFmpeg scale filter that outputs exactly 1080×1920.
///
/// The filter chain:
/// 1. Scales to target dimensions using high-quality Lanczos interpolation
/// 2. Sets SAR to 1:1 for square pixels
///
/// # Example Output
/// ```text
/// scale=1080:1920:flags=lanczos,setsar=1
/// ```
#[inline]
pub fn portrait_scale_filter() -> String {
    format!(
        "scale={}:{}:flags=lanczos,setsar=1",
        PORTRAIT_WIDTH, PORTRAIT_HEIGHT
    )
}

/// Builds an FFmpeg scale filter for split-view panels (1080×960 each).
///
/// Uses force_original_aspect_ratio=decrease to avoid stretching,
/// then pads to exact dimensions with centered content.
///
/// # Example Output  
/// ```text
/// scale=1080:960:flags=lanczos:force_original_aspect_ratio=decrease,pad=1080:960:(ow-iw)/2:(oh-ih)/2,setsar=1
/// ```
#[inline]
pub fn split_panel_scale_filter() -> String {
    format!(
        "scale={}:{}:flags=lanczos:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
        SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT,
        SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT
    )
}

/// Ensures crop dimensions are even (required by libx264).
///
/// Rounds down to the nearest even number.
#[inline]
pub fn make_even(value: i32) -> i32 {
    (value / 2) * 2
}

/// Clamps crop coordinates to ensure they stay within frame bounds.
///
/// Returns (x, y, width, height) with:
/// - Even dimensions for codec compatibility
/// - Coordinates clamped to prevent out-of-bounds access
pub fn clamp_crop_to_frame(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    frame_width: u32,
    frame_height: u32,
) -> (i32, i32, i32, i32) {
    // Ensure even dimensions
    let w = make_even(width.min(frame_width as i32));
    let h = make_even(height.min(frame_height as i32));
    
    // Clamp position to keep crop within frame
    let x = x.max(0).min((frame_width as i32) - w);
    let y = y.max(0).min((frame_height as i32) - h);
    
    (x.max(0), y.max(0), w.max(2), h.max(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portrait_dimensions() {
        assert_eq!(PORTRAIT_WIDTH, 1080);
        assert_eq!(PORTRAIT_HEIGHT, 1920);
        // Verify 9:16 aspect ratio
        let ratio = PORTRAIT_WIDTH as f64 / PORTRAIT_HEIGHT as f64;
        assert!((ratio - 0.5625).abs() < 0.001);
    }

    #[test]
    fn test_split_dimensions() {
        assert_eq!(SPLIT_PANEL_WIDTH, 1080);
        assert_eq!(SPLIT_PANEL_HEIGHT, 960);
        // Two panels should equal portrait height
        assert_eq!(SPLIT_PANEL_HEIGHT * 2, PORTRAIT_HEIGHT);
    }

    #[test]
    fn test_make_even() {
        assert_eq!(make_even(1080), 1080);
        assert_eq!(make_even(1081), 1080);
        assert_eq!(make_even(1079), 1078);
        assert_eq!(make_even(1), 0);
        assert_eq!(make_even(2), 2);
    }

    #[test]
    fn test_clamp_crop_to_frame() {
        // Normal case - crop fits within frame
        let (x, y, w, h) = clamp_crop_to_frame(100, 100, 500, 800, 1920, 1080);
        assert_eq!((x, y, w, h), (100, 100, 500, 800));

        // Crop extends beyond right edge
        let (x, y, w, h) = clamp_crop_to_frame(1500, 100, 500, 800, 1920, 1080);
        assert_eq!(x, 1920 - 500); // Pushed left
        assert_eq!(w, 500);

        // Crop extends beyond bottom
        let (x, y, w, h) = clamp_crop_to_frame(100, 500, 500, 800, 1920, 1080);
        assert_eq!(y, 1080 - 800); // Pushed up

        // Crop larger than frame
        let (x, y, w, h) = clamp_crop_to_frame(0, 0, 2000, 2000, 1920, 1080);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
        assert_eq!(x, 0);
        assert_eq!(y, 0);

        // Odd dimensions should be made even
        let (x, y, w, h) = clamp_crop_to_frame(0, 0, 501, 801, 1920, 1080);
        assert_eq!(w, 500);
        assert_eq!(h, 800);
    }

    #[test]
    fn test_portrait_scale_filter() {
        let filter = portrait_scale_filter();
        assert!(filter.contains("scale=1080:1920"));
        assert!(filter.contains("lanczos"));
        assert!(filter.contains("setsar=1"));
    }

    #[test]
    fn test_split_panel_scale_filter() {
        let filter = split_panel_scale_filter();
        assert!(filter.contains("scale=1080:960"));
        assert!(filter.contains("pad=1080:960"));
        assert!(filter.contains("setsar=1"));
    }
}
