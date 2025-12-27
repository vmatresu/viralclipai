//! Shot boundary detection using color histogram comparison.
//!
//! Implements AutoAI-style scene detection by comparing color histograms
//! of consecutive frames. When the histogram difference exceeds a threshold,
//! a shot boundary is detected.
//!
//! # Algorithm
//!
//! 1. Extract frames at sample FPS (e.g., 5 FPS)
//! 2. Compute HSV histogram for each frame (16 bins per channel)
//! 3. Compare consecutive histograms using chi-squared distance
//! 4. Mark shot boundary when distance > threshold
//!
//! # References
//!
//! - AutoAI uses similar histogram-based shot detection
//! - Chi-squared distance is robust to gradual illumination changes

use std::path::Path;
use tracing::{debug, info};

/// A detected shot (scene) in the video.
#[derive(Debug, Clone)]
pub struct Shot {
    /// Zero-based index of first frame in the shot.
    pub start_frame: usize,
    /// Zero-based index of last frame in the shot (inclusive).
    pub end_frame: usize,
    /// Start time in seconds.
    pub start_time: f64,
    /// End time in seconds.
    pub end_time: f64,
}

impl Shot {
    /// Duration of the shot in seconds.
    pub fn duration(&self) -> f64 {
        self.end_time - self.start_time
    }

    /// Number of frames in the shot.
    pub fn frame_count(&self) -> usize {
        self.end_frame - self.start_frame + 1
    }
}

/// Shot boundary detector using color histogram comparison.
///
/// Detects hard cuts by comparing HSV color histograms of consecutive frames.
/// When the chi-squared distance exceeds the threshold, a shot boundary is marked.
pub struct ShotDetector {
    /// Chi-squared distance threshold for shot boundary (default: 0.5).
    /// Higher values = fewer detected cuts, lower = more sensitive.
    threshold: f64,

    /// Minimum number of frames per shot (default: 15 at 30fps = 0.5s).
    /// Prevents detecting rapid flashes as separate shots.
    min_shot_frames: usize,

    /// Number of histogram bins per HSV channel.
    histogram_bins: usize,
}

impl Default for ShotDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotDetector {
    /// Create a new shot detector with default settings.
    pub fn new() -> Self {
        Self {
            threshold: 0.5,
            min_shot_frames: 15,
            histogram_bins: 16,
        }
    }

    /// Create with custom threshold.
    ///
    /// # Arguments
    /// * `threshold` - Chi-squared distance threshold (0.3-1.0 typical)
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Set minimum shot duration in frames.
    pub fn with_min_frames(mut self, min_frames: usize) -> Self {
        self.min_shot_frames = min_frames;
        self
    }

    /// Detect shots from pre-extracted frame histograms.
    ///
    /// # Arguments
    /// * `histograms` - HSV histograms for each frame (flattened to 1D)
    /// * `fps` - Frames per second for time calculation
    ///
    /// # Returns
    /// Vector of detected shots covering the entire video.
    pub fn detect_from_histograms(&self, histograms: &[Vec<f64>], fps: f64) -> Vec<Shot> {
        if histograms.is_empty() {
            return vec![];
        }

        if histograms.len() == 1 {
            return vec![Shot {
                start_frame: 0,
                end_frame: 0,
                start_time: 0.0,
                end_time: 1.0 / fps,
            }];
        }

        let mut boundaries: Vec<usize> = vec![0]; // Always start with frame 0

        // Compare consecutive histograms
        for i in 1..histograms.len() {
            let distance = self.chi_squared_distance(&histograms[i - 1], &histograms[i]);

            if distance > self.threshold {
                // Check minimum shot duration from last boundary
                let last_boundary = *boundaries.last().unwrap();
                if i - last_boundary >= self.min_shot_frames {
                    debug!(
                        "Shot boundary at frame {} (distance={:.3}, threshold={:.3})",
                        i, distance, self.threshold
                    );
                    boundaries.push(i);
                } else {
                    debug!(
                        "Ignoring potential boundary at frame {} (too close to previous: {} frames)",
                        i,
                        i - last_boundary
                    );
                }
            }
        }

        // Convert boundaries to shots
        let mut shots = Vec::with_capacity(boundaries.len());
        for (idx, &start_frame) in boundaries.iter().enumerate() {
            let end_frame = if idx + 1 < boundaries.len() {
                boundaries[idx + 1] - 1
            } else {
                histograms.len() - 1
            };

            shots.push(Shot {
                start_frame,
                end_frame,
                start_time: start_frame as f64 / fps,
                end_time: (end_frame + 1) as f64 / fps,
            });
        }

        info!(
            "Detected {} shots from {} frames",
            shots.len(),
            histograms.len()
        );
        shots
    }

    /// Compute chi-squared distance between two histograms.
    ///
    /// Chi-squared is robust to gradual illumination changes and
    /// works well for shot boundary detection.
    ///
    /// Formula: sum((h1[i] - h2[i])^2 / (h1[i] + h2[i] + epsilon))
    fn chi_squared_distance(&self, h1: &[f64], h2: &[f64]) -> f64 {
        const EPSILON: f64 = 1e-10;

        if h1.len() != h2.len() {
            return f64::MAX;
        }

        let mut distance = 0.0;
        for (a, b) in h1.iter().zip(h2.iter()) {
            let sum = a + b + EPSILON;
            let diff = a - b;
            distance += (diff * diff) / sum;
        }

        distance / 2.0 // Normalize
    }

    /// Compute HSV histogram for an RGB frame.
    ///
    /// # Arguments
    /// * `rgb_data` - Raw RGB pixel data (3 bytes per pixel)
    /// * `width` - Frame width
    /// * `height` - Frame height
    ///
    /// # Returns
    /// Flattened histogram with `bins^3` entries.
    pub fn compute_histogram(&self, rgb_data: &[u8], width: u32, height: u32) -> Vec<f64> {
        let bins = self.histogram_bins;
        let total_bins = bins * bins * bins;
        let mut histogram = vec![0.0; total_bins];

        let pixel_count = (width * height) as usize;
        let expected_len = pixel_count * 3;

        if rgb_data.len() < expected_len {
            return histogram;
        }

        for i in 0..pixel_count {
            let r = rgb_data[i * 3] as f64 / 255.0;
            let g = rgb_data[i * 3 + 1] as f64 / 255.0;
            let b = rgb_data[i * 3 + 2] as f64 / 255.0;

            let (h, s, v) = rgb_to_hsv(r, g, b);

            // Quantize to bins
            let h_bin = ((h / 360.0) * bins as f64).min(bins as f64 - 1.0) as usize;
            let s_bin = (s * bins as f64).min(bins as f64 - 1.0) as usize;
            let v_bin = (v * bins as f64).min(bins as f64 - 1.0) as usize;

            let idx = h_bin * bins * bins + s_bin * bins + v_bin;
            histogram[idx] += 1.0;
        }

        // Normalize
        let total: f64 = histogram.iter().sum();
        if total > 0.0 {
            for val in &mut histogram {
                *val /= total;
            }
        }

        histogram
    }
}

/// Convert RGB to HSV color space.
///
/// # Arguments
/// * `r`, `g`, `b` - RGB values in [0, 1]
///
/// # Returns
/// (H, S, V) where H is in [0, 360), S and V are in [0, 1]
fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    // Value
    let v = max;

    // Saturation
    let s = if max == 0.0 { 0.0 } else { delta / max };

    // Hue
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    (h, s, v)
}

/// Extract frames from video and compute histograms.
///
/// This is the main entry point for shot detection from a video file.
#[cfg(feature = "opencv")]
#[allow(dead_code)]
pub async fn extract_histograms_from_video<P: AsRef<Path>>(
    video_path: P,
    start_time: f64,
    end_time: f64,
    sample_fps: f64,
) -> crate::error::MediaResult<(Vec<Vec<f64>>, f64)> {
    use opencv::prelude::*;
    use opencv::videoio::{VideoCapture, CAP_PROP_POS_MSEC};

    let video_path = video_path.as_ref();
    let video_str = video_path.to_str().unwrap_or("");

    let mut cap = VideoCapture::from_file(video_str, opencv::videoio::CAP_ANY).map_err(|e| {
        crate::error::MediaError::detection_failed(format!("Failed to open video: {}", e))
    })?;

    let sample_interval = 1.0 / sample_fps;
    let duration = end_time - start_time;
    let num_samples = (duration / sample_interval).ceil() as usize;

    let detector = ShotDetector::new();
    let mut histograms = Vec::with_capacity(num_samples);
    let mut current_time = start_time;

    for _ in 0..num_samples {
        if current_time >= end_time {
            break;
        }

        // Seek to time
        if cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0).is_err() {
            current_time += sample_interval;
            continue;
        }

        // Read frame
        let mut frame = opencv::core::Mat::default();
        if !cap.read(&mut frame).unwrap_or(false) || frame.empty() {
            current_time += sample_interval;
            continue;
        }

        // Convert to RGB and compute histogram
        let mut rgb_frame = opencv::core::Mat::default();
        opencv::imgproc::cvt_color_def(&frame, &mut rgb_frame, opencv::imgproc::COLOR_BGR2RGB)
            .map_err(|e| {
                crate::error::MediaError::detection_failed(format!(
                    "Color conversion failed: {}",
                    e
                ))
            })?;

        let width = rgb_frame.cols() as u32;
        let height = rgb_frame.rows() as u32;
        let data = rgb_frame.data_bytes().map_err(|e| {
            crate::error::MediaError::detection_failed(format!("Failed to get frame data: {}", e))
        })?;

        let histogram = detector.compute_histogram(data, width, height);
        histograms.push(histogram);

        current_time += sample_interval;
    }

    Ok((histograms, sample_fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chi_squared_identical() {
        let detector = ShotDetector::new();
        let h1 = vec![0.25, 0.25, 0.25, 0.25];
        let h2 = vec![0.25, 0.25, 0.25, 0.25];
        let distance = detector.chi_squared_distance(&h1, &h2);
        assert!(
            distance < 0.001,
            "Identical histograms should have ~0 distance"
        );
    }

    #[test]
    fn test_chi_squared_different() {
        let detector = ShotDetector::new();
        let h1 = vec![1.0, 0.0, 0.0, 0.0];
        let h2 = vec![0.0, 0.0, 0.0, 1.0];
        let distance = detector.chi_squared_distance(&h1, &h2);
        assert!(
            distance > 0.5,
            "Completely different histograms should have high distance"
        );
    }

    #[test]
    fn test_no_shots_uniform() {
        let detector = ShotDetector::new();
        // Same histogram repeated = no cuts
        let histograms: Vec<Vec<f64>> = (0..30).map(|_| vec![0.25, 0.25, 0.25, 0.25]).collect();

        let shots = detector.detect_from_histograms(&histograms, 30.0);
        assert_eq!(shots.len(), 1, "Uniform content should be 1 shot");
        assert_eq!(shots[0].start_frame, 0);
        assert_eq!(shots[0].end_frame, 29);
    }

    #[test]
    fn test_hard_cut_detection() {
        let detector = ShotDetector::new().with_threshold(0.3).with_min_frames(5);

        // First 15 frames: all red
        // Last 15 frames: all blue
        let mut histograms = Vec::new();
        for _ in 0..15 {
            histograms.push(vec![1.0, 0.0, 0.0, 0.0]);
        }
        for _ in 0..15 {
            histograms.push(vec![0.0, 0.0, 0.0, 1.0]);
        }

        let shots = detector.detect_from_histograms(&histograms, 30.0);
        assert_eq!(shots.len(), 2, "Should detect 2 shots");
        assert_eq!(shots[0].start_frame, 0);
        assert_eq!(shots[0].end_frame, 14);
        assert_eq!(shots[1].start_frame, 15);
        assert_eq!(shots[1].end_frame, 29);
    }

    #[test]
    fn test_min_shot_duration() {
        // With min_shot_frames=15, boundaries must be at least 15 frames apart
        let detector = ShotDetector::new().with_threshold(0.3).with_min_frames(15);

        // Create a pattern with cuts at frames 10 and 20:
        // - Frame 10 cut: 10 - 0 = 10 frames from start, < 15, so FILTERED
        // - Frame 20 cut: 20 - 0 = 20 frames from start, >= 15, so ACCEPTED
        let mut histograms = Vec::new();

        // Frames 0-9: red (10 frames)
        for _ in 0..10 {
            histograms.push(vec![1.0, 0.0, 0.0, 0.0]);
        }
        // Frames 10-19: blue (10 frames)
        for _ in 0..10 {
            histograms.push(vec![0.0, 0.0, 0.0, 1.0]);
        }
        // Frames 20-29: green (10 frames)
        for _ in 0..10 {
            histograms.push(vec![0.0, 1.0, 0.0, 0.0]);
        }

        let shots = detector.detect_from_histograms(&histograms, 30.0);
        // Frame 10 cut is filtered (only 10 frames from 0), but frame 20 cut is accepted
        // Result: shots starting at frame 0 and frame 20
        assert_eq!(
            shots.len(),
            2,
            "Should have 2 shots (cut at frame 10 filtered, cut at frame 20 accepted)"
        );
        assert_eq!(shots[0].start_frame, 0);
        assert_eq!(shots[0].end_frame, 19); // First shot spans frames 0-19
        assert_eq!(shots[1].start_frame, 20);
        assert_eq!(shots[1].end_frame, 29); // Second shot spans frames 20-29
    }

    #[test]
    fn test_rgb_to_hsv_red() {
        let (h, s, v) = rgb_to_hsv(1.0, 0.0, 0.0);
        assert!((h - 0.0).abs() < 1.0, "Red hue should be ~0");
        assert!((s - 1.0).abs() < 0.01, "Pure red should have saturation 1");
        assert!((v - 1.0).abs() < 0.01, "Pure red should have value 1");
    }

    #[test]
    fn test_rgb_to_hsv_green() {
        let (h, s, v) = rgb_to_hsv(0.0, 1.0, 0.0);
        assert!((h - 120.0).abs() < 1.0, "Green hue should be ~120");
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_blue() {
        let (h, s, v) = rgb_to_hsv(0.0, 0.0, 1.0);
        assert!((h - 240.0).abs() < 1.0, "Blue hue should be ~240");
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_gray() {
        let (_h, s, v) = rgb_to_hsv(0.5, 0.5, 0.5);
        assert!((s - 0.0).abs() < 0.01, "Gray should have saturation 0");
        assert!((v - 0.5).abs() < 0.01, "Gray should have value 0.5");
    }

    #[test]
    fn test_shot_duration() {
        let shot = Shot {
            start_frame: 0,
            end_frame: 29,
            start_time: 0.0,
            end_time: 1.0,
        };
        assert!((shot.duration() - 1.0).abs() < 0.001);
        assert_eq!(shot.frame_count(), 30);
    }

    #[test]
    fn test_empty_histograms() {
        let detector = ShotDetector::new();
        let shots = detector.detect_from_histograms(&[], 30.0);
        assert!(shots.is_empty());
    }

    #[test]
    fn test_single_frame() {
        let detector = ShotDetector::new();
        let histograms = vec![vec![0.25, 0.25, 0.25, 0.25]];
        let shots = detector.detect_from_histograms(&histograms, 30.0);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].start_frame, 0);
        assert_eq!(shots[0].end_frame, 0);
    }
}
