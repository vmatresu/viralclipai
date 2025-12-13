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
    ///
    /// Uses "zoom to fill" strategy: crops a region that exactly matches the target
    /// aspect ratio, ensuring no black bars and no stretching. The crop is centered
    /// on the focus point (face) and fills the entire output frame.
    ///
    /// IMPORTANT: Face centering is applied AFTER zoom computation to ensure
    /// the face remains properly centered in the final output, not cut off.
    fn keyframe_to_crop(
        &self,
        keyframe: &CameraKeyframe,
        aspect_ratio: &AspectRatio,
    ) -> CropWindow {
        let target_ratio = aspect_ratio.ratio();

        // ZOOM TO FILL: Compute the largest crop region that:
        // 1. Has exactly the target aspect ratio (no black bars, no stretching)
        // 2. Fits within the source frame
        // 3. Is centered on the focus point
        let (crop_width, crop_height) = self.compute_zoom_to_fill_crop(keyframe, target_ratio);

        // CRITICAL: Center on face AFTER zoom computation
        // The face center (cx, cy) should be positioned in the upper-third of the crop
        // for optimal framing (rule of thirds / TikTok style)

        // Target: place face center at ~35% from top of crop (upper third)
        // This gives natural headroom above the face
        let target_face_y_ratio = 0.35;

        // Compute where the crop should be positioned to achieve this framing
        let mut x = keyframe.cx - crop_width / 2.0;
        let mut y = keyframe.cy - crop_height * target_face_y_ratio;

        // Clamp to frame boundaries FIRST
        x = x.max(0.0).min(self.frame_width as f64 - crop_width);
        y = y.max(0.0).min(self.frame_height as f64 - crop_height);

        // After clamping, verify the face is still within the crop
        // If the face would be cut off, adjust the crop to include it
        let face_top = keyframe.cy - keyframe.height / 2.0;
        let face_bottom = keyframe.cy + keyframe.height / 2.0;
        let crop_top = y;
        let crop_bottom = y + crop_height;

        // Ensure face is not cut off at top
        if face_top < crop_top {
            y = (face_top - crop_height * 0.05).max(0.0); // 5% margin
        }
        // Ensure face is not cut off at bottom
        if face_bottom > crop_bottom {
            y = (face_bottom - crop_height + crop_height * 0.05)
                .min(self.frame_height as f64 - crop_height);
        }

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

    /// Compute crop dimensions using "zoom to fill" strategy.
    ///
    /// This ensures the crop region has EXACTLY the target aspect ratio,
    /// eliminating black bars entirely. The crop is sized to:
    /// 1. Contain the focus region with margins
    /// 2. Not exceed max zoom factor
    /// 3. Fit within the source frame
    fn compute_zoom_to_fill_crop(
        &self,
        keyframe: &CameraKeyframe,
        target_ratio: f64,
    ) -> (f64, f64) {
        let source_ratio = self.frame_width as f64 / self.frame_height as f64;
        let focus_height = keyframe.height;
        let focus_width = keyframe.width;
        let min_margin = self.config.safe_margin;

        // Calculate minimum crop size to contain focus region with margins
        let required_height = focus_height * (1.0 + 2.0 * min_margin);
        let required_width = focus_width * (1.0 + 2.0 * min_margin);

        // Determine crop dimensions that exactly match target aspect ratio
        let (mut crop_width, mut crop_height) = if target_ratio <= source_ratio {
            // Target is narrower than source (e.g., 9:16 from 16:9)
            // Height-constrained: use full height, crop width
            let h = self.frame_height as f64;
            let w = h * target_ratio;

            // But ensure we contain the focus region
            if required_height > h {
                // Focus is taller than frame - use full height
                (w, h)
            } else if required_width > w {
                // Focus is wider than our narrow crop - expand height to fit
                let expanded_w = required_width;
                let expanded_h = expanded_w / target_ratio;
                if expanded_h <= self.frame_height as f64 {
                    (expanded_w, expanded_h)
                } else {
                    // Can't fit - use max possible
                    (w, h)
                }
            } else {
                // Start with minimum size that contains focus
                let min_h = required_height;
                let min_w = min_h * target_ratio;

                // Apply zoom limits
                let zoom_factor = self.frame_height as f64 / min_h;
                if zoom_factor > self.config.max_zoom_factor {
                    let limited_h = self.frame_height as f64 / self.config.max_zoom_factor;
                    let limited_w = limited_h * target_ratio;
                    (
                        limited_w.min(self.frame_width as f64),
                        limited_h.min(self.frame_height as f64),
                    )
                } else {
                    (
                        min_w.min(self.frame_width as f64),
                        min_h.min(self.frame_height as f64),
                    )
                }
            }
        } else {
            // Target is wider than source
            // Width-constrained: use full width, crop height
            let w = self.frame_width as f64;
            let h = w / target_ratio;

            if required_width > w {
                (w, h)
            } else if required_height > h {
                let expanded_h = required_height;
                let expanded_w = expanded_h * target_ratio;
                if expanded_w <= self.frame_width as f64 {
                    (expanded_w, expanded_h)
                } else {
                    (w, h)
                }
            } else {
                let min_w = required_width;
                let min_h = min_w / target_ratio;

                let zoom_factor = self.frame_height as f64 / min_h;
                if zoom_factor > self.config.max_zoom_factor {
                    let limited_h = self.frame_height as f64 / self.config.max_zoom_factor;
                    let limited_w = limited_h * target_ratio;
                    (
                        limited_w.min(self.frame_width as f64),
                        limited_h.min(self.frame_height as f64),
                    )
                } else {
                    (
                        min_w.min(self.frame_width as f64),
                        min_h.min(self.frame_height as f64),
                    )
                }
            }
        };

        // Final clamp to frame bounds while maintaining aspect ratio
        if crop_width > self.frame_width as f64 {
            crop_width = self.frame_width as f64;
            crop_height = crop_width / target_ratio;
        }
        if crop_height > self.frame_height as f64 {
            crop_height = self.frame_height as f64;
            crop_width = crop_height * target_ratio;
        }

        (crop_width, crop_height)
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
            let t =
                (time - crop_windows[i].time) / (crop_windows[i + 1].time - crop_windows[i].time);
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
    fn test_face_not_cut_off_at_top() {
        let config = IntelligentCropConfig::default();
        let planner = CropPlanner::new(config, 1920, 1080);

        // Face near top of frame
        let keyframe = CameraKeyframe::new(0.0, 960.0, 100.0, 200.0, 150.0);
        let aspect = AspectRatio::PORTRAIT;

        let crop = planner.keyframe_to_crop(&keyframe, &aspect);

        // Face top should be within crop (with margin)
        let face_top = keyframe.cy - keyframe.height / 2.0;
        let crop_top = crop.y as f64;
        assert!(
            face_top >= crop_top,
            "Face top ({}) should be >= crop top ({})",
            face_top,
            crop_top
        );
    }

    #[test]
    fn test_face_not_cut_off_at_bottom() {
        let config = IntelligentCropConfig::default();
        let planner = CropPlanner::new(config, 1920, 1080);

        // Face near bottom of frame
        let keyframe = CameraKeyframe::new(0.0, 960.0, 950.0, 200.0, 150.0);
        let aspect = AspectRatio::PORTRAIT;

        let crop = planner.keyframe_to_crop(&keyframe, &aspect);

        // Face bottom should be within crop (with margin)
        let face_bottom = keyframe.cy + keyframe.height / 2.0;
        let crop_bottom = (crop.y + crop.height) as f64;
        assert!(
            face_bottom <= crop_bottom,
            "Face bottom ({}) should be <= crop bottom ({})",
            face_bottom,
            crop_bottom
        );
    }

    #[test]
    fn test_face_centered_in_upper_third() {
        let config = IntelligentCropConfig::default();
        let planner = CropPlanner::new(config, 1920, 1080);

        // Face in center of frame
        let keyframe = CameraKeyframe::new(0.0, 960.0, 540.0, 200.0, 200.0);
        let aspect = AspectRatio::PORTRAIT;

        let crop = planner.keyframe_to_crop(&keyframe, &aspect);

        // Face center should be in upper portion of crop (20-50% from top)
        let face_cy = keyframe.cy;
        let crop_top = crop.y as f64;
        let crop_height = crop.height as f64;
        let face_position_ratio = (face_cy - crop_top) / crop_height;

        assert!(
            face_position_ratio >= 0.2 && face_position_ratio <= 0.5,
            "Face should be in upper third, but position ratio is {}",
            face_position_ratio
        );
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

    #[test]
    fn test_crop_stays_within_frame_bounds() {
        let config = IntelligentCropConfig::default();
        let planner = CropPlanner::new(config, 1920, 1080);

        // Test various edge positions
        let test_cases = vec![
            (100.0, 100.0),  // Top-left
            (1800.0, 100.0), // Top-right
            (100.0, 980.0),  // Bottom-left
            (1800.0, 980.0), // Bottom-right
            (960.0, 540.0),  // Center
        ];

        for (cx, cy) in test_cases {
            let keyframe = CameraKeyframe::new(0.0, cx, cy, 200.0, 200.0);
            let crop = planner.keyframe_to_crop(&keyframe, &AspectRatio::PORTRAIT);

            assert!(crop.x >= 0, "Crop x ({}) should be >= 0", crop.x);
            assert!(crop.y >= 0, "Crop y ({}) should be >= 0", crop.y);
            assert!(
                crop.x + crop.width <= 1920,
                "Crop right edge ({}) should be <= 1920",
                crop.x + crop.width
            );
            assert!(
                crop.y + crop.height <= 1080,
                "Crop bottom edge ({}) should be <= 1080",
                crop.y + crop.height
            );
        }
    }
}
