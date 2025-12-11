//! Crop window computation for premium intelligent_speaker style.
//!
//! Extracted from camera_planner.rs for better modularity and testability.
//! Handles aspect-ratio aware framing with vertical bias and zoom limits.

use crate::intelligent::models::{AspectRatio, CameraKeyframe, CropWindow};

/// Configuration for crop computation.
#[derive(Debug, Clone, Copy)]
pub struct CropComputeConfig {
    /// Headroom ratio as fraction of crop height
    pub headroom_ratio: f64,
    /// Safe margin from crop edge
    pub safe_margin: f64,
    /// Maximum zoom factor
    pub max_zoom_factor: f64,
    /// Minimum zoom factor
    pub min_zoom_factor: f64,
}

impl Default for CropComputeConfig {
    fn default() -> Self {
        Self {
            headroom_ratio: 0.12,
            safe_margin: 0.05,
            max_zoom_factor: 2.5,
            min_zoom_factor: 1.0,
        }
    }
}

impl From<&super::config::PremiumSpeakerConfig> for CropComputeConfig {
    fn from(config: &super::config::PremiumSpeakerConfig) -> Self {
        Self {
            headroom_ratio: config.headroom_ratio,
            safe_margin: config.safe_margin,
            max_zoom_factor: config.max_zoom_factor,
            min_zoom_factor: config.min_zoom_factor,
        }
    }
}

/// Computes crop windows from camera keyframes with aspect-ratio awareness.
///
/// Responsibilities:
/// - Convert camera keyframes to FFmpeg-compatible crop windows
/// - Maintain target aspect ratio
/// - Apply headroom and zoom constraints
/// - Ensure crops stay within frame bounds
pub struct CropComputer {
    config: CropComputeConfig,
    frame_width: u32,
    frame_height: u32,
}

impl CropComputer {
    /// Create a new crop computer.
    pub fn new(config: CropComputeConfig, frame_width: u32, frame_height: u32) -> Self {
        Self {
            config,
            frame_width,
            frame_height,
        }
    }

    /// Compute crop windows from camera keyframes.
    pub fn compute_crop_windows(
        &self,
        keyframes: &[CameraKeyframe],
        target_aspect: &AspectRatio,
    ) -> Vec<CropWindow> {
        keyframes
            .iter()
            .map(|kf| self.keyframe_to_crop(kf, target_aspect))
            .collect()
    }

    /// Convert a single camera keyframe to a crop window.
    pub fn keyframe_to_crop(
        &self,
        keyframe: &CameraKeyframe,
        aspect_ratio: &AspectRatio,
    ) -> CropWindow {
        let target_ratio = aspect_ratio.ratio();
        let source_ratio = self.frame_width as f64 / self.frame_height as f64;

        // Determine crop dimensions based on aspect ratios
        let (crop_width, crop_height) = if target_ratio <= source_ratio {
            self.compute_narrow_crop(keyframe, target_ratio)
        } else {
            self.compute_wide_crop(keyframe, target_ratio)
        };

        // Compute crop position centered on focus point
        let (x, y) = self.compute_crop_position(keyframe, crop_width, crop_height);

        // Ensure even dimensions (required by video codecs)
        let width = Self::make_even(crop_width.round() as i32);
        let height = Self::make_even(crop_height.round() as i32);

        // Final bounds check
        let x = x.min(self.frame_width as i32 - width).max(0);
        let y = y.min(self.frame_height as i32 - height).max(0);

        CropWindow::new(keyframe.time, x, y, width.max(2), height.max(2))
    }

    /// Compute crop position with headroom adjustment.
    fn compute_crop_position(
        &self,
        keyframe: &CameraKeyframe,
        crop_width: f64,
        crop_height: f64,
    ) -> (i32, i32) {
        let mut x = keyframe.cx - crop_width / 2.0;
        let mut y = keyframe.cy - crop_height / 2.0;

        // Apply headroom adjustment (shift crop up to give headroom above face)
        let headroom_shift = crop_height * self.config.headroom_ratio * 0.15;
        y -= headroom_shift;

        // Clamp to frame boundaries
        x = x.max(0.0).min(self.frame_width as f64 - crop_width);
        y = y.max(0.0).min(self.frame_height as f64 - crop_height);

        (x.round() as i32, y.round() as i32)
    }

    /// Compute crop for narrow target (e.g., 9:16 portrait from 16:9 landscape).
    fn compute_narrow_crop(&self, keyframe: &CameraKeyframe, target_ratio: f64) -> (f64, f64) {
        let focus_height = keyframe.height;
        let min_margin = self.config.safe_margin;

        let required_height = focus_height * (1.0 + 2.0 * min_margin);
        let required_width = required_height * target_ratio;

        let (crop_width, crop_height) = self.clamp_to_frame(required_width, required_height, target_ratio);

        // Apply zoom limits
        let zoom_factor = self.frame_width as f64 / crop_width;
        if zoom_factor > self.config.max_zoom_factor {
            let w = self.frame_width as f64 / self.config.max_zoom_factor;
            let h = w / target_ratio;
            return (w, h.min(self.frame_height as f64));
        }

        let final_height = crop_height.min(self.frame_height as f64);
        let final_width = (final_height * target_ratio).min(self.frame_width as f64);

        (final_width, final_height)
    }

    /// Compute crop for wide target.
    fn compute_wide_crop(&self, keyframe: &CameraKeyframe, target_ratio: f64) -> (f64, f64) {
        let focus_width = keyframe.width;
        let min_margin = self.config.safe_margin;

        let required_width = focus_width * (1.0 + 2.0 * min_margin);
        let required_height = required_width / target_ratio;

        let (crop_width, _) = self.clamp_to_frame(required_width, required_height, target_ratio);

        let final_width = crop_width.min(self.frame_width as f64);
        let final_height = (final_width / target_ratio).min(self.frame_height as f64);

        (final_width, final_height)
    }

    /// Clamp dimensions to frame bounds while maintaining aspect ratio.
    fn clamp_to_frame(&self, width: f64, height: f64, target_ratio: f64) -> (f64, f64) {
        if width > self.frame_width as f64 {
            let w = self.frame_width as f64;
            let h = w / target_ratio;
            (w, h)
        } else if height > self.frame_height as f64 {
            let h = self.frame_height as f64;
            let w = h * target_ratio;
            (w, h)
        } else {
            (width, height)
        }
    }

    /// Make a value even (required by video codecs).
    #[inline]
    fn make_even(value: i32) -> i32 {
        (value / 2) * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_narrow_crop_aspect_ratio() {
        let config = CropComputeConfig::default();
        let computer = CropComputer::new(config, 1920, 1080);

        let keyframe = CameraKeyframe::new(0.0, 960.0, 540.0, 200.0, 300.0);
        let crop = computer.keyframe_to_crop(&keyframe, &AspectRatio::PORTRAIT);

        // Should be 9:16 aspect ratio
        let ratio = crop.width as f64 / crop.height as f64;
        assert!((ratio - 0.5625).abs() < 0.02, "Aspect ratio wrong: {}", ratio);
    }

    #[test]
    fn test_crop_within_bounds() {
        let config = CropComputeConfig::default();
        let computer = CropComputer::new(config, 1920, 1080);

        // Test edge case: subject near corner
        let keyframe = CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 300.0);
        let crop = computer.keyframe_to_crop(&keyframe, &AspectRatio::PORTRAIT);

        assert!(crop.x >= 0);
        assert!(crop.y >= 0);
        assert!(crop.x + crop.width <= 1920);
        assert!(crop.y + crop.height <= 1080);
    }

    #[test]
    fn test_even_dimensions() {
        let config = CropComputeConfig::default();
        let computer = CropComputer::new(config, 1920, 1080);

        let keyframe = CameraKeyframe::new(0.0, 960.0, 540.0, 201.0, 301.0);
        let crop = computer.keyframe_to_crop(&keyframe, &AspectRatio::PORTRAIT);

        assert_eq!(crop.width % 2, 0, "Width must be even");
        assert_eq!(crop.height % 2, 0, "Height must be even");
    }

    #[test]
    fn test_zoom_limit_enforced() {
        let mut config = CropComputeConfig::default();
        config.max_zoom_factor = 2.0;

        let computer = CropComputer::new(config, 1920, 1080);

        // Very small focus region should be limited by max zoom
        let keyframe = CameraKeyframe::new(0.0, 960.0, 540.0, 50.0, 50.0);
        let crop = computer.keyframe_to_crop(&keyframe, &AspectRatio::PORTRAIT);

        let zoom = 1920.0 / crop.width as f64;
        assert!(zoom <= 2.1, "Zoom {} exceeds limit", zoom);
    }
}
