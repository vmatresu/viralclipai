//! Crop window computation for different aspect ratios.
//!
//! This module converts camera keyframes to actual crop windows
//! while maintaining proper composition and aspect ratios.

use super::config::IntelligentCropConfig;
use super::models::{AspectRatio, CameraKeyframe, CropWindow};

/// Crop planner for computing crop windows.
pub struct CropPlanner {
    config: IntelligentCropConfig,
    frame_width: u32,
    frame_height: u32,
}

impl CropPlanner {
    /// Create a new crop planner.
    pub fn new(config: IntelligentCropConfig, frame_width: u32, frame_height: u32) -> Self {
        Self {
            config,
            frame_width,
            frame_height,
        }
    }

    /// Compute crop windows for camera keyframes.
    ///
    /// # Arguments
    /// * `keyframes` - Camera keyframes with focus positions
    /// * `aspect_ratio` - Target aspect ratio
    ///
    /// # Returns
    /// Crop windows ready for FFmpeg rendering
    pub fn compute_crop_windows(
        &self,
        keyframes: &[CameraKeyframe],
        aspect_ratio: &AspectRatio,
    ) -> Vec<CropWindow> {
        keyframes
            .iter()
            .map(|kf| self.keyframe_to_crop(kf, aspect_ratio))
            .collect()
    }

    /// Convert a camera keyframe to a crop window.
    fn keyframe_to_crop(&self, keyframe: &CameraKeyframe, aspect_ratio: &AspectRatio) -> CropWindow {
        let target_ratio = aspect_ratio.ratio();
        let source_ratio = self.frame_width as f64 / self.frame_height as f64;

        // Determine crop dimensions based on aspect ratios
        let (crop_width, crop_height) = if target_ratio <= source_ratio {
            // Target is narrower than source (e.g., 9:16 from 16:9)
            self.compute_narrow_crop(keyframe, target_ratio)
        } else {
            // Target is wider than source
            self.compute_wide_crop(keyframe, target_ratio)
        };

        // Compute crop position centered on focus point
        let mut x = keyframe.cx - crop_width / 2.0;
        let mut y = keyframe.cy - crop_height / 2.0;

        // Apply headroom adjustment for faces
        let headroom_shift = crop_height * self.config.headroom_ratio * 0.3;
        y -= headroom_shift;

        // Clamp to frame boundaries
        x = x.max(0.0).min(self.frame_width as f64 - crop_width);
        y = y.max(0.0).min(self.frame_height as f64 - crop_height);

        // Ensure integer values and even dimensions (required by many codecs)
        let x = (x.round() as i32).max(0);
        let y = (y.round() as i32).max(0);
        let width = ((crop_width.round() as i32) / 2) * 2; // Make even
        let height = ((crop_height.round() as i32) / 2) * 2; // Make even

        // Final bounds check
        let x = x.min(self.frame_width as i32 - width);
        let y = y.min(self.frame_height as i32 - height);

        CropWindow::new(
            keyframe.time,
            x.max(0),
            y.max(0),
            width.max(2),
            height.max(2),
        )
    }

    /// Compute crop dimensions for narrow target (e.g., 9:16).
    fn compute_narrow_crop(&self, keyframe: &CameraKeyframe, target_ratio: f64) -> (f64, f64) {
        let focus_height = keyframe.height;
        let min_margin = self.config.safe_margin;

        // Compute minimum crop that contains focus region
        let required_height = focus_height * (1.0 + 2.0 * min_margin);
        let required_width = required_height * target_ratio;

        let (crop_width, crop_height) = if required_width > self.frame_width as f64 {
            // Width limited - use full width
            let w = self.frame_width as f64;
            let h = w / target_ratio;
            (w, h)
        } else if required_height > self.frame_height as f64 {
            // Height limited - use full height
            let h = self.frame_height as f64;
            let w = h * target_ratio;
            (w, h)
        } else {
            (required_width, required_height)
        };

        // Apply zoom limits
        let zoom_factor = self.frame_width as f64 / crop_width;
        if zoom_factor > self.config.max_zoom_factor {
            let w = self.frame_width as f64 / self.config.max_zoom_factor;
            let h = w / target_ratio;
            return (w, h.min(self.frame_height as f64));
        }

        // Ensure crop fits in frame
        let final_height = crop_height.min(self.frame_height as f64);
        let final_width = (final_height * target_ratio).min(self.frame_width as f64);

        (final_width, final_height)
    }

    /// Compute crop dimensions for wide target.
    fn compute_wide_crop(&self, keyframe: &CameraKeyframe, target_ratio: f64) -> (f64, f64) {
        let focus_width = keyframe.width;
        let min_margin = self.config.safe_margin;

        let required_width = focus_width * (1.0 + 2.0 * min_margin);
        let required_height = required_width / target_ratio;

        let (crop_width, _crop_height) = if required_width > self.frame_width as f64 {
            let w = self.frame_width as f64;
            let h = w / target_ratio;
            (w, h)
        } else if required_height > self.frame_height as f64 {
            let h = self.frame_height as f64;
            let w = h * target_ratio;
            (w, h)
        } else {
            (required_width, required_height)
        };

        // Ensure crop fits in frame
        let final_width = crop_width.min(self.frame_width as f64);
        let final_height = (final_width / target_ratio).min(self.frame_height as f64);

        (final_width, final_height)
    }
}

/// Interpolate crop window at a specific time.
pub fn interpolate_crop_window(crop_windows: &[CropWindow], time: f64) -> Option<CropWindow> {
    if crop_windows.is_empty() {
        return None;
    }

    // Handle edge cases
    if time <= crop_windows[0].time {
        return Some(crop_windows[0]);
    }
    if time >= crop_windows.last().unwrap().time {
        return Some(*crop_windows.last().unwrap());
    }

    // Find surrounding keyframes
    for i in 0..crop_windows.len() - 1 {
        if crop_windows[i].time <= time && time <= crop_windows[i + 1].time {
            let t = (time - crop_windows[i].time) / (crop_windows[i + 1].time - crop_windows[i].time);
            return Some(CropWindow::lerp(&crop_windows[i], &crop_windows[i + 1], t));
        }
    }

    None
}

/// Check if crop windows are essentially static.
pub fn is_static_crop(crop_windows: &[CropWindow]) -> bool {
    if crop_windows.len() <= 1 {
        return true;
    }

    let x_vals: Vec<i32> = crop_windows.iter().map(|w| w.x).collect();
    let y_vals: Vec<i32> = crop_windows.iter().map(|w| w.y).collect();
    let w_vals: Vec<i32> = crop_windows.iter().map(|w| w.width).collect();

    let x_range = x_vals.iter().max().unwrap() - x_vals.iter().min().unwrap();
    let y_range = y_vals.iter().max().unwrap() - y_vals.iter().min().unwrap();
    let w_range = w_vals.iter().max().unwrap() - w_vals.iter().min().unwrap();

    let avg_width = w_vals.iter().sum::<i32>() as f64 / w_vals.len() as f64;

    // Consider static if movement is less than 5% of width
    let threshold = (avg_width * 0.05) as i32;
    x_range < threshold && y_range < threshold && w_range < threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_narrow_crop_from_landscape() {
        let config = IntelligentCropConfig::default();
        let planner = CropPlanner::new(config, 1920, 1080);

        let keyframe = CameraKeyframe::new(0.0, 960.0, 540.0, 200.0, 300.0);
        let aspect = AspectRatio::PORTRAIT; // 9:16

        let crop = planner.keyframe_to_crop(&keyframe, &aspect);

        // Should produce portrait crop
        assert!(crop.height > crop.width);
        // Aspect ratio should be close to 9:16
        let ratio = crop.width as f64 / crop.height as f64;
        assert!((ratio - 0.5625).abs() < 0.01);
    }

    #[test]
    fn test_static_crop_detection() {
        let static_windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 102, 101, 500, 500),
            CropWindow::new(2.0, 101, 100, 500, 500),
        ];
        assert!(is_static_crop(&static_windows));

        let moving_windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 200, 200, 500, 500),
            CropWindow::new(2.0, 300, 300, 500, 500),
        ];
        assert!(!is_static_crop(&moving_windows));
    }

    #[test]
    fn test_interpolation() {
        let windows = vec![
            CropWindow::new(0.0, 0, 0, 100, 100),
            CropWindow::new(1.0, 100, 100, 200, 200),
        ];

        let mid = interpolate_crop_window(&windows, 0.5).unwrap();
        assert_eq!(mid.x, 50);
        assert_eq!(mid.y, 50);
        assert_eq!(mid.width, 150);
    }
}
