//! Lightweight heuristic motion detection for Tier 1 (MotionAware).
//! Uses frame differencing on a downscaled grid to find the center of motion.

#[cfg(feature = "opencv")]
use opencv::{
    core::{Point, Size},
    imgproc,
    prelude::*,
};

#[cfg(feature = "opencv")]
use crate::error::{MediaError, MediaResult};

/// Simple frame-diff motion detector.
#[cfg(feature = "opencv")]
pub struct MotionDetector {
    /// Previous frame (downscaled, gray) for differencing.
    prev_frame: Option<Mat>,
    /// Processing resolution (small grid for speed, e.g., 64x36).
    proc_size: Size,
    /// Minimum pixel intensity change to count as motion (0-255).
    threshold: f64,
}

#[cfg(feature = "opencv")]
impl MotionDetector {
    /// Create a new detector sized to the source frame.
    pub fn new(width: i32, height: i32) -> Self {
        // Scale down to ~64px width while maintaining aspect ratio.
        let scale = 64.0 / width.max(1) as f64;
        let proc_width = 64;
        let proc_height = ((height as f64 * scale).max(1.0)) as i32;

        Self {
            prev_frame: None,
            proc_size: Size::new(proc_width, proc_height),
            threshold: 25.0, // Ignore subtle noise
        }
    }

    /// Detect the center of motion in the frame.
    /// Returns the center point in original frame coordinates, or None if no significant motion.
    pub fn detect_center(&mut self, frame: &Mat) -> MediaResult<Option<Point>> {
        // 1) Resize & grayscale (fast).
        let mut small = Mat::default();
        imgproc::resize(
            frame,
            &mut small,
            self.proc_size,
            0.0,
            0.0,
            imgproc::INTER_NEAREST,
        )
        .map_err(|e| MediaError::detection_failed(format!("motion resize: {e}")))?;

        let mut gray = Mat::default();
        if small.channels() == 3 {
            imgproc::cvt_color(
                &small,
                &mut gray,
                imgproc::COLOR_BGR2GRAY,
                0,
                opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
            )
            .map_err(|e| MediaError::detection_failed(format!("motion bgr2gray: {e}")))?;
        } else {
            gray = small;
        }

        // 2) Frame difference.
        let result = if let Some(ref prev) = self.prev_frame {
            let mut diff = Mat::default();
            opencv::core::absdiff(prev, &gray, &mut diff)
                .map_err(|e| MediaError::detection_failed(format!("motion absdiff: {e}")))?;

            let mut thresh = Mat::default();
            imgproc::threshold(
                &diff,
                &mut thresh,
                self.threshold,
                255.0,
                imgproc::THRESH_BINARY,
            )
            .map_err(|e| MediaError::detection_failed(format!("motion threshold: {e}")))?;

            // 3) Center of mass (weighted average of white pixels).
            let moments = imgproc::moments(&thresh, true)
                .map_err(|e| MediaError::detection_failed(format!("motion moments: {e}")))?;
            if moments.m00 > 10.0 {
                let cx = (moments.m10 / moments.m00) as i32;
                let cy = (moments.m01 / moments.m00) as i32;

                // Map back to full resolution.
                let scale_x = frame.cols() as f64 / self.proc_size.width as f64;
                let scale_y = frame.rows() as f64 / self.proc_size.height as f64;

                Some(Point::new(
                    (cx as f64 * scale_x) as i32,
                    (cy as f64 * scale_y) as i32,
                ))
            } else {
                None
            }
        } else {
            None
        };

        // Store current frame for next iteration.
        self.prev_frame = Some(gray);
        Ok(result)
    }
}
