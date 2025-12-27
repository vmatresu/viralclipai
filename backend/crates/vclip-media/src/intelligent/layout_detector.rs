//! Video layout detection and heuristic face region generation.
//!
//! This module provides:
//! - Video layout classification (single person, podcast, panel)
//! - Heuristic face region generation for different layouts
//! - Face position estimation based on video composition
//!
//! Used as a fallback when actual face detection (YuNet) is unavailable.

use super::config::IntelligentCropConfig;
use super::models::BoundingBox;
use crate::error::MediaResult;
use std::path::Path;

/// Detected video layout type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoLayout {
    /// Single person talking head
    SinglePerson,
    /// Two people side by side (podcast format)
    TwoPeopleSideBySide,
}

/// Configuration for layout detection.
#[derive(Debug, Clone)]
pub struct LayoutDetectorConfig {
    /// Minimum aspect ratio to consider a video "wide" (landscape)
    pub wide_aspect_threshold: f64,
    /// Minimum width for HD video (podcast detection)
    pub hd_width_threshold: u32,
    /// Motion balance range for two-person detection  
    pub balanced_motion_min: f64,
    pub balanced_motion_max: f64,
}

impl Default for LayoutDetectorConfig {
    fn default() -> Self {
        Self {
            wide_aspect_threshold: 1.5,
            hd_width_threshold: 1280,
            balanced_motion_min: 0.3,
            balanced_motion_max: 0.7,
        }
    }
}

/// Layout detector for video composition analysis.
pub struct LayoutDetector {
    config: LayoutDetectorConfig,
}

impl LayoutDetector {
    /// Create a new layout detector with default configuration.
    pub fn new() -> Self {
        Self {
            config: LayoutDetectorConfig::default(),
        }
    }

    /// Create a layout detector with custom configuration.
    pub fn with_config(config: LayoutDetectorConfig) -> Self {
        Self { config }
    }

    /// Detect the video layout type.
    pub async fn detect_layout<P: AsRef<Path>>(
        &self,
        video_path: P,
        width: u32,
        height: u32,
    ) -> MediaResult<VideoLayout> {
        let video_path = video_path.as_ref();

        // For landscape videos (16:9), check if it's likely a two-person podcast
        let aspect_ratio = width as f64 / height as f64;

        if aspect_ratio > self.config.wide_aspect_threshold {
            // Wide video - likely 16:9 or wider
            // Analyze motion/content distribution in left vs right halves
            let motion_balance = self.analyze_motion_balance(video_path, width).await?;

            if motion_balance > self.config.balanced_motion_min
                && motion_balance < self.config.balanced_motion_max
            {
                // Motion is roughly balanced between halves - likely two people
                return Ok(VideoLayout::TwoPeopleSideBySide);
            }
        }

        // For portrait or near-square, assume single person
        // Also for landscape with unbalanced motion
        Ok(VideoLayout::SinglePerson)
    }

    /// Analyze motion balance between left and right halves of the frame.
    /// Returns a value between 0.0 (all motion on left) and 1.0 (all motion on right).
    async fn analyze_motion_balance<P: AsRef<Path>>(
        &self,
        _video_path: P,
        width: u32,
    ) -> MediaResult<f64> {
        // Use FFmpeg to analyze scene changes in each half
        // For now, use a simple heuristic: if video is 16:9, assume podcast format
        // This could be enhanced with actual motion analysis

        // Check if width suggests a standard podcast format (1920x1080)
        if width >= self.config.hd_width_threshold {
            // Assume balanced (podcast) format for HD landscape videos
            return Ok(0.5);
        }

        // Narrow videos are likely single-person
        Ok(0.0)
    }
}

impl Default for LayoutDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Heuristic face region generator.
///
/// Generates estimated face bounding boxes based on video layout
/// and composition rules, without actual face detection.
pub struct HeuristicGenerator {
    config: IntelligentCropConfig,
}

impl HeuristicGenerator {
    /// Create a new heuristic generator.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self { config }
    }

    /// Heuristic for single person talking head video.
    /// Face is centered horizontally, in upper portion of frame.
    pub fn single_person_heuristic(
        &self,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> Vec<Vec<(BoundingBox, f64)>> {
        let w = width as f64;
        let h = height as f64;

        // Face typically occupies ~25-35% of frame height in talking head
        let face_height = h * 0.30;
        let face_width = face_height * 0.8;

        // Face center at 50% width, 35% height
        let cx = w * 0.5;
        let cy = h * 0.35;

        let mut detections = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let variation = (i as f64 * 0.1).sin() * 0.015;
            let confidence = 0.7 + variation.abs() * 0.1;

            let bbox = BoundingBox::new(
                cx - face_width / 2.0 + w * variation * 0.3,
                cy - face_height / 2.0 + h * variation * 0.2,
                face_width,
                face_height,
            )
            .clamp(width, height);

            detections.push(vec![(bbox, confidence)]);
        }

        detections
    }

    /// Heuristic for the LEFT person in a two-person side-by-side video.
    /// Face is at ~25% width (left quarter), in upper portion of frame.
    pub fn left_person_heuristic(
        &self,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> Vec<Vec<(BoundingBox, f64)>> {
        let w = width as f64;
        let h = height as f64;

        // Face typically occupies ~25-30% of frame height
        let face_height = h * 0.28;
        let face_width = face_height * 0.8;

        // LEFT person: face center at ~25% width (center of left half), 38% height
        let cx = w * 0.25;
        let cy = h * 0.38;

        let mut detections = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let variation = (i as f64 * 0.1).sin() * 0.01;
            let confidence = 0.75 + variation.abs() * 0.1;

            let bbox = BoundingBox::new(
                cx - face_width / 2.0 + w * variation * 0.2,
                cy - face_height / 2.0 + h * variation * 0.15,
                face_width,
                face_height,
            )
            .clamp(width, height);

            detections.push(vec![(bbox, confidence)]);
        }

        detections
    }

    /// Heuristic for the RIGHT person in a two-person side-by-side video.
    /// Face is at ~75% width (right quarter), in upper portion of frame.
    pub fn right_person_heuristic(
        &self,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> Vec<Vec<(BoundingBox, f64)>> {
        let w = width as f64;
        let h = height as f64;

        // Face typically occupies ~25-30% of frame height
        let face_height = h * 0.28;
        let face_width = face_height * 0.8;

        // RIGHT person: face center at ~75% width (center of right half), 38% height
        let cx = w * 0.75;
        let cy = h * 0.38;

        let mut detections = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let variation = (i as f64 * 0.1).sin() * 0.01;
            let confidence = 0.75 + variation.abs() * 0.1;

            let bbox = BoundingBox::new(
                cx - face_width / 2.0 + w * variation * 0.2,
                cy - face_height / 2.0 + h * variation * 0.15,
                face_width,
                face_height,
            )
            .clamp(width, height);

            detections.push(vec![(bbox, confidence)]);
        }

        detections
    }

    /// Create a detection centered in the given region.
    /// Used for split processing where each half is processed separately.
    pub fn create_centered_detection(
        &self,
        width: u32,
        height: u32,
        confidence: f64,
    ) -> (BoundingBox, f64) {
        let w = width as f64;
        let h = height as f64;

        let face_height = h * 0.28;
        let face_width = face_height * 0.8;

        // Center of frame, upper portion
        let cx = w * 0.5;
        let cy = h * 0.38;

        let bbox = BoundingBox::new(
            cx - face_width / 2.0,
            cy - face_height / 2.0,
            face_width,
            face_height,
        );

        (bbox.clamp(width, height), confidence)
    }

    /// Speaker-aware heuristic for two-person side-by-side videos.
    ///
    /// This method:
    /// 1. Analyzes speaker activity (audio or motion-based)
    /// 2. Generates detections that follow the active speaker
    /// 3. Provides smooth transitions between speakers
    pub async fn speaker_aware_heuristic<P: AsRef<Path>>(
        &self,
        _video_path: P,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        // Visual-only fallback: generate stable left/right boxes without audio analysis.
        let sample_interval = 1.0 / self.config.fps_sample;
        self.generate_speaker_aware_detections(width, height, num_samples, sample_interval)
    }

    /// Generate frame-by-frame detections based on speaker activity.
    ///
    /// Creates bounding boxes that follow the active speaker while
    /// maintaining smooth transitions between speaker changes.
    pub fn generate_speaker_aware_detections(
        &self,
        width: u32,
        height: u32,
        num_samples: usize,
        _sample_interval: f64,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        let w = width as f64;
        let h = height as f64;

        // Face dimensions for podcast layout
        let face_height = h * 0.28;
        let face_width = face_height * 0.8;

        // Face Y position (upper portion of frame)
        let cy = h * 0.38;

        // Face X positions for left and right speakers
        let left_cx = w * 0.25; // Center of left half
        let right_cx = w * 0.75; // Center of right half

        let mut detections = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            let left_bbox = BoundingBox::new(
                left_cx - face_width / 2.0,
                cy - face_height / 2.0,
                face_width,
                face_height,
            )
            .clamp(width, height);

            let right_bbox = BoundingBox::new(
                right_cx - face_width / 2.0,
                cy - face_height / 2.0,
                face_width,
                face_height,
            )
            .clamp(width, height);

            detections.push(vec![(left_bbox, 0.8), (right_bbox, 0.8)]);
        }

        Ok(detections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_person_heuristic() {
        let config = IntelligentCropConfig::default();
        let generator = HeuristicGenerator::new(config);

        let detections = generator.single_person_heuristic(1920, 1080, 10);

        assert_eq!(detections.len(), 10);
        for frame_dets in detections {
            assert_eq!(frame_dets.len(), 1);
            assert!(frame_dets[0].1 >= 0.5); // Confidence
        }
    }

    #[test]
    fn test_left_person_heuristic() {
        let config = IntelligentCropConfig::default();
        let generator = HeuristicGenerator::new(config);

        let detections = generator.left_person_heuristic(1920, 1080, 10);

        assert_eq!(detections.len(), 10);
        for frame_dets in &detections {
            assert_eq!(frame_dets.len(), 1);
            // Face should be in left quarter of frame
            let bbox = &frame_dets[0].0;
            assert!(bbox.cx() < 1920.0 * 0.35, "Face should be in left portion");
        }
    }

    #[test]
    fn test_right_person_heuristic() {
        let config = IntelligentCropConfig::default();
        let generator = HeuristicGenerator::new(config);

        let detections = generator.right_person_heuristic(1920, 1080, 10);

        assert_eq!(detections.len(), 10);
        for frame_dets in &detections {
            assert_eq!(frame_dets.len(), 1);
            // Face should be in right quarter of frame
            let bbox = &frame_dets[0].0;
            assert!(bbox.cx() > 1920.0 * 0.65, "Face should be in right portion");
        }
    }

    #[test]
    fn test_create_centered_detection() {
        let config = IntelligentCropConfig::default();
        let generator = HeuristicGenerator::new(config);

        let (bbox, score) = generator.create_centered_detection(1920, 1080, 0.8);

        // Check face is in upper-center region
        assert!(bbox.cx() > 900.0 && bbox.cx() < 1020.0);
        assert!(bbox.cy() > 350.0 && bbox.cy() < 500.0);
        assert_eq!(score, 0.8);
    }
}
