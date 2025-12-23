//! Scene Cut Detection for Temporal Decimation
//!
//! Detects scene cuts (shot boundaries) using color histogram comparison.
//! Scene cuts trigger tracker reset to prevent ghost detections.
//!
//! # Algorithm
//! 1. Compute color histogram for each frame (8 bins per channel = 512 bins)
//! 2. Compare consecutive histograms using histogram intersection or chi-square
//! 3. If similarity drops below threshold, declare scene cut
//!
//! # Integration with Tracking
//! When a scene cut is detected, the Kalman tracker MUST hard-reset to prevent
//! tracks from "ghosting" across cuts (e.g., Person A → Person B).
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::scene_cut::SceneCutDetector;
//!
//! let mut detector = SceneCutDetector::new(0.3);
//! let is_cut = detector.check_frame(&frame);
//! if is_cut {
//!     tracker.hard_reset();
//! }
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{debug, info};

#[cfg(feature = "opencv")]
use opencv::{
    core::{Mat, Vector},
    imgproc,
    prelude::*,
};

/// Configuration for scene cut detection.
#[derive(Debug, Clone)]
pub struct SceneCutConfig {
    /// Similarity threshold (0.0-1.0)
    /// Below this = scene cut
    pub threshold: f64,

    /// Number of histogram bins per channel
    pub bins_per_channel: i32,

    /// Minimum frames between cuts (debounce)
    pub min_cut_interval: u32,

    /// Use downscaled frame for faster histogram computation
    pub downsample_factor: u32,
}

impl Default for SceneCutConfig {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            bins_per_channel: 8,
            min_cut_interval: 5,
            downsample_factor: 4,
        }
    }
}

/// Scene cut detector using histogram comparison.
pub struct SceneCutDetector {
    config: SceneCutConfig,
    /// Previous frame histogram (flattened)
    prev_histogram: Option<Vec<f32>>,
    /// Previous frame hash for quick comparison
    prev_hash: u64,
    /// Frame count since last cut
    frames_since_cut: u32,
    /// Total cuts detected
    cut_count: u64,
}

impl SceneCutDetector {
    /// Create a new scene cut detector with given threshold.
    pub fn new(threshold: f64) -> Self {
        Self {
            config: SceneCutConfig {
                threshold,
                ..Default::default()
            },
            prev_histogram: None,
            prev_hash: 0,
            frames_since_cut: 0,
            cut_count: 0,
        }
    }

    /// Create with full configuration.
    pub fn with_config(config: SceneCutConfig) -> Self {
        Self {
            config,
            prev_histogram: None,
            prev_hash: 0,
            frames_since_cut: 0,
            cut_count: 0,
        }
    }

    /// Check if current frame is a scene cut.
    ///
    /// # Arguments
    /// * `frame` - Current frame (BGR)
    ///
    /// # Returns
    /// `true` if scene cut detected, `false` otherwise
    #[cfg(feature = "opencv")]
    pub fn check_frame(&mut self, frame: &Mat) -> bool {
        if frame.empty() {
            return false;
        }

        // Compute histogram
        let histogram = match self.compute_histogram(frame) {
            Ok(h) => h,
            Err(e) => {
                debug!("Histogram computation failed: {}", e);
                return false;
            }
        };

        // First frame - no comparison possible
        let prev = match &self.prev_histogram {
            Some(h) => h,
            None => {
                self.prev_histogram = Some(histogram);
                self.frames_since_cut = 0;
                return false;
            }
        };

        // Check debounce
        if self.frames_since_cut < self.config.min_cut_interval {
            self.frames_since_cut += 1;
            self.prev_histogram = Some(histogram);
            return false;
        }

        // Compare histograms
        let similarity = histogram_intersection(prev, &histogram);
        let is_cut = similarity < self.config.threshold;

        if is_cut {
            info!(
                similarity = format!("{:.3}", similarity),
                threshold = self.config.threshold,
                "Scene cut detected"
            );
            self.cut_count += 1;
            self.frames_since_cut = 0;
        } else {
            self.frames_since_cut += 1;
        }

        self.prev_histogram = Some(histogram);
        is_cut
    }

    /// Get scene hash for current frame.
    ///
    /// Used for tracker scene awareness. Different hash = different scene.
    #[cfg(feature = "opencv")]
    pub fn compute_scene_hash(&self, frame: &Mat) -> u64 {
        if frame.empty() {
            return 0;
        }

        // Quick hash using downsampled pixel values
        match self.compute_quick_hash(frame) {
            Ok(h) => h,
            Err(_) => 0,
        }
    }

    /// Reset detector state for new video.
    pub fn reset(&mut self) {
        self.prev_histogram = None;
        self.prev_hash = 0;
        self.frames_since_cut = 0;
    }

    /// Get total cuts detected.
    pub fn cut_count(&self) -> u64 {
        self.cut_count
    }

    /// Compute color histogram for frame.
    #[cfg(feature = "opencv")]
    fn compute_histogram(&self, frame: &Mat) -> Result<Vec<f32>, opencv::Error> {
        let bins = self.config.bins_per_channel;
        let total_bins = (bins * bins * bins) as usize;

        // Downsample for speed
        let downsampled = if self.config.downsample_factor > 1 {
            let mut small = Mat::default();
            let factor = self.config.downsample_factor as f64;
            imgproc::resize(
                frame,
                &mut small,
                opencv::core::Size::new(
                    (frame.cols() as f64 / factor) as i32,
                    (frame.rows() as f64 / factor) as i32,
                ),
                0.0,
                0.0,
                imgproc::INTER_NEAREST,
            )?;
            small
        } else {
            frame.clone()
        };

        // Convert to HSV for better scene comparison
        let mut hsv = Mat::default();
        imgproc::cvt_color_def(&downsampled, &mut hsv, imgproc::COLOR_BGR2HSV)?;

        // Compute histogram
        let channels = Vector::from_slice(&[0, 1]); // H and S channels
        let hist_size = Vector::from_slice(&[bins, bins]);
        // Flat ranges array: [h_min, h_max, s_min, s_max]
        let ranges = Vector::from_slice(&[0.0f32, 180.0, 0.0f32, 256.0]);

        let mut hist = Mat::default();
        // Build Vector<Mat> using push since from_slice doesn't work for Mat
        let mut images: Vector<Mat> = Vector::new();
        images.push(hsv);
        let mask = Mat::default();

        imgproc::calc_hist(
            &images,
            &channels,
            &mask,
            &mut hist,
            &hist_size,
            &ranges,
            false,
        )?;

        // Normalize
        let mut hist_norm = Mat::default();
        opencv::core::normalize(
            &hist,
            &mut hist_norm,
            0.0,
            1.0,
            opencv::core::NORM_MINMAX,
            -1,
            &Mat::default(),
        )?;

        // Flatten to vector
        let mut result = vec![0.0f32; (bins * bins) as usize];
        for i in 0..bins {
            for j in 0..bins {
                let val = *hist_norm.at_2d::<f32>(i, j)?;
                result[(i * bins + j) as usize] = val;
            }
        }

        Ok(result)
    }

    /// Compute quick hash for scene identification.
    #[cfg(feature = "opencv")]
    fn compute_quick_hash(&self, frame: &Mat) -> Result<u64, opencv::Error> {
        // Downsample to 8x8
        let mut tiny = Mat::default();
        imgproc::resize(
            frame,
            &mut tiny,
            opencv::core::Size::new(8, 8),
            0.0,
            0.0,
            imgproc::INTER_AREA,
        )?;

        // Hash the pixel values
        let mut hasher = DefaultHasher::new();
        for y in 0..8 {
            for x in 0..8 {
                let pixel = tiny.at_2d::<opencv::core::Vec3b>(y, x)?;
                (pixel[0] as u64).hash(&mut hasher);
                (pixel[1] as u64).hash(&mut hasher);
                (pixel[2] as u64).hash(&mut hasher);
            }
        }

        Ok(hasher.finish())
    }

    /// Non-opencv stub
    #[cfg(not(feature = "opencv"))]
    pub fn check_frame(&mut self, _frame: &()) -> bool {
        false
    }

    #[cfg(not(feature = "opencv"))]
    pub fn compute_scene_hash(&self, _frame: &()) -> u64 {
        0
    }
}

/// Compute histogram intersection (similarity measure).
///
/// Returns value in [0, 1] where 1 = identical histograms.
fn histogram_intersection(h1: &[f32], h2: &[f32]) -> f64 {
    if h1.len() != h2.len() || h1.is_empty() {
        return 0.0;
    }

    let mut intersection = 0.0f64;
    let mut sum1 = 0.0f64;
    let mut sum2 = 0.0f64;

    for (a, b) in h1.iter().zip(h2.iter()) {
        intersection += (*a as f64).min(*b as f64);
        sum1 += *a as f64;
        sum2 += *b as f64;
    }

    let denominator = sum1.min(sum2);
    if denominator > 0.0 {
        intersection / denominator
    } else {
        0.0
    }
}

/// Compute chi-square distance between histograms.
///
/// Returns distance where 0 = identical, higher = more different.
#[allow(dead_code)]
fn histogram_chi_square(h1: &[f32], h2: &[f32]) -> f64 {
    if h1.len() != h2.len() || h1.is_empty() {
        return f64::MAX;
    }

    let mut chi_sq = 0.0f64;
    for (a, b) in h1.iter().zip(h2.iter()) {
        let sum = *a as f64 + *b as f64;
        if sum > 0.0 {
            let diff = *a as f64 - *b as f64;
            chi_sq += (diff * diff) / sum;
        }
    }

    chi_sq
}

/// Quick scene hash from raw bytes (for non-OpenCV use).
pub fn compute_scene_hash_from_bytes(pixels: &[u8], width: usize, height: usize) -> u64 {
    if pixels.is_empty() || width == 0 || height == 0 {
        return 0;
    }

    // Sample pixels at regular intervals
    let mut hasher = DefaultHasher::new();
    let stride = (width * height * 3) / 64; // Sample ~64 points

    for i in (0..pixels.len()).step_by(stride.max(1)) {
        pixels[i].hash(&mut hasher);
    }

    hasher.finish()
}

/// Check if scene changed based on hash comparison.
pub fn is_scene_cut_by_hash(prev_hash: u64, curr_hash: u64, threshold: f64) -> bool {
    if prev_hash == 0 || curr_hash == 0 {
        return false;
    }

    // XOR-based similarity
    let diff_bits = (prev_hash ^ curr_hash).count_ones();
    let similarity = 1.0 - (diff_bits as f64 / 64.0);

    similarity < threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_intersection_identical() {
        let h = vec![0.25, 0.25, 0.25, 0.25];
        let similarity = histogram_intersection(&h, &h);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_histogram_intersection_different() {
        let h1 = vec![1.0, 0.0, 0.0, 0.0];
        let h2 = vec![0.0, 0.0, 0.0, 1.0];
        let similarity = histogram_intersection(&h1, &h2);
        assert!(similarity < 0.01);
    }

    #[test]
    fn test_scene_cut_by_hash() {
        // Same hash
        assert!(!is_scene_cut_by_hash(12345, 12345, 0.3));

        // Very different hash
        assert!(is_scene_cut_by_hash(0x0000_0000_0000_0000, 0xFFFF_FFFF_FFFF_FFFF, 0.3));
    }

    #[test]
    fn test_config_defaults() {
        let config = SceneCutConfig::default();
        assert_eq!(config.bins_per_channel, 8);
        assert_eq!(config.min_cut_interval, 5);
    }

    #[test]
    fn test_detector_creation() {
        let detector = SceneCutDetector::new(0.5);
        assert_eq!(detector.cut_count(), 0);
    }

    #[test]
    fn test_scene_hash_from_bytes() {
        let pixels = vec![128u8; 100 * 100 * 3];
        let hash = compute_scene_hash_from_bytes(&pixels, 100, 100);
        assert!(hash != 0);

        // Same pixels should give same hash
        let hash2 = compute_scene_hash_from_bytes(&pixels, 100, 100);
        assert_eq!(hash, hash2);
    }
}
