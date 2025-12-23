//! Detection Adapter - Converts cached neural analysis to detection formats.
//!
//! This module provides a clean interface for converting between the cached
//! `SceneNeuralAnalysis` format and the internal detection formats used by
//! the cropper and split processors.
//!
//! # Architecture
//!
//! The adapter decouples detection from rendering by providing:
//! - Conversion from cached analysis to internal detection format
//! - Centralized fallback detection when cache is unavailable
//! - Consistent interface for all intelligent styles
//!
//! # Usage
//!
//! All intelligent processors should use this adapter instead of running
//! detection directly:
//!
//! ```ignore
//! // Preferred: use cached analysis
//! let detections = if let Some(analysis) = cached_analysis {
//!     convert_cached_analysis(analysis, width, height)
//! } else {
//!     // Fallback: run detection through adapter
//!     run_fallback_detection(video_path, tier, start, end, width, height, fps).await?
//! };
//! ```

use std::path::Path;
use tracing::{info, warn};
use vclip_models::{DetectionTier, SceneNeuralAnalysis};

use super::config::{FaceEngineMode, IntelligentCropConfig};
use super::detector::FaceDetector;
use super::models::{BoundingBox, Detection};
use super::optimized_detector::OptimizedFaceDetector;
use crate::detection::pipeline_builder::PipelineBuilder;
use crate::error::MediaResult;

/// Converts cached neural analysis to the detection format used by croppers.
///
/// This is the primary interface for consuming cached analysis. All intelligent
/// styles should use this instead of running detection directly.
pub fn convert_cached_analysis(
    analysis: &SceneNeuralAnalysis,
    width: u32,
    height: u32,
) -> Vec<Vec<Detection>> {
    let fw = width as f32;
    let fh = height as f32;

    analysis
        .frames
        .iter()
        .map(|frame| {
            frame
                .faces
                .iter()
                .map(|face| {
                    let (x, y, w, h) = face.bbox.to_pixels(fw, fh);
                    let bbox = BoundingBox::new(x as f64, y as f64, w as f64, h as f64);
                    Detection::with_mouth(
                        frame.time,
                        bbox,
                        face.score as f64,
                        face.track_id.unwrap_or(0),
                        face.mouth_openness.map(|m| m as f64),
                    )
                })
                .collect()
        })
        .collect()
}

/// Extracts split layout information from cached analysis.
///
/// Returns (should_split, left_detections, right_detections) where:
/// - should_split: true if split layout is appropriate
/// - left_detections: faces in left half of frame
/// - right_detections: faces in right half of frame
pub fn extract_split_info(
    analysis: &SceneNeuralAnalysis,
    width: u32,
    height: u32,
    duration: f64,
) -> SplitLayoutInfo {
    const MIN_SIMULTANEOUS_SECONDS: f64 = 3.0;

    if analysis.frames.is_empty() {
        return SplitLayoutInfo::no_split();
    }

    let center_x = width as f64 / 2.0;
    let sample_interval = duration / analysis.frames.len().max(1) as f64;
    let fw = width as f32;
    let fh = height as f32;

    let mut simultaneous_time = 0.0;
    let mut distinct_tracks = std::collections::HashSet::new();
    let mut left_faces: Vec<BoundingBox> = Vec::new();
    let mut right_faces: Vec<BoundingBox> = Vec::new();

    for frame in &analysis.frames {
        if frame.faces.len() >= 2 {
            simultaneous_time += sample_interval;
        }
        for face in &frame.faces {
            if let Some(track_id) = face.track_id {
                distinct_tracks.insert(track_id);
            }
            let (x, y, w, h) = face.bbox.to_pixels(fw, fh);
            let bbox = BoundingBox::new(x as f64, y as f64, w as f64, h as f64);
            if bbox.cx() < center_x {
                left_faces.push(bbox);
            } else {
                right_faces.push(bbox);
            }
        }
    }

    let should_split = distinct_tracks.len() >= 2 && simultaneous_time >= MIN_SIMULTANEOUS_SECONDS;

    info!(
        "[SPLIT_ADAPTER] {} tracks, {:.1}s simultaneous (need >= {:.1}s) → {}",
        distinct_tracks.len(),
        simultaneous_time,
        MIN_SIMULTANEOUS_SECONDS,
        if should_split { "SPLIT" } else { "FULL-FRAME" }
    );

    SplitLayoutInfo {
        should_split,
        simultaneous_time,
        distinct_tracks: distinct_tracks.len(),
        left_faces,
        right_faces,
    }
}

/// Information about split layout suitability.
#[derive(Debug)]
pub struct SplitLayoutInfo {
    pub should_split: bool,
    pub simultaneous_time: f64,
    pub distinct_tracks: usize,
    pub left_faces: Vec<BoundingBox>,
    pub right_faces: Vec<BoundingBox>,
}

impl SplitLayoutInfo {
    /// Create a default no-split layout.
    pub fn no_split() -> Self {
        Self {
            should_split: false,
            simultaneous_time: 0.0,
            distinct_tracks: 0,
            left_faces: Vec::new(),
            right_faces: Vec::new(),
        }
    }

    /// Create a default split layout (used when detection fails).
    pub fn default_split() -> Self {
        Self {
            should_split: true,
            simultaneous_time: 0.0,
            distinct_tracks: 0,
            left_faces: Vec::new(),
            right_faces: Vec::new(),
        }
    }

    /// Compute vertical bias for left panel (0.0 = top, 1.0 = bottom).
    pub fn left_vertical_bias(&self, height: u32) -> f64 {
        compute_vertical_bias(&self.left_faces, height)
    }

    /// Compute vertical bias for right panel (0.0 = top, 1.0 = bottom).
    pub fn right_vertical_bias(&self, height: u32) -> f64 {
        compute_vertical_bias(&self.right_faces, height)
    }

    /// Compute horizontal center for left panel (0.0-1.0 within left half).
    pub fn left_horizontal_center(&self, width: u32) -> f64 {
        let half_width = width as f64 / 2.0;
        if self.left_faces.is_empty() {
            0.5
        } else {
            let avg_cx: f64 =
                self.left_faces.iter().map(|f| f.cx()).sum::<f64>() / self.left_faces.len() as f64;
            (avg_cx / half_width).clamp(0.1, 0.9)
        }
    }

    /// Compute horizontal center for right panel (0.0-1.0 within right half).
    pub fn right_horizontal_center(&self, width: u32) -> f64 {
        let half_width = width as f64 / 2.0;
        if self.right_faces.is_empty() {
            0.5
        } else {
            let avg_cx: f64 = self.right_faces.iter().map(|f| f.cx()).sum::<f64>()
                / self.right_faces.len() as f64;
            ((avg_cx - half_width) / half_width).clamp(0.1, 0.9)
        }
    }
}

/// Compute vertical bias from face positions, ensuring faces are never cut.
///
/// Returns a value from 0.0 (crop at top) to ~0.5 (crop at bottom).
/// The bias is used as: `crop_y = vertical_margin * bias`
///
/// **Key principle**: Preserve headroom above faces. Cut empty space, not faces.
/// When faces are high in the frame, we should crop from the bottom (low bias),
/// not from the top which would cut off foreheads/scalps.
pub fn compute_vertical_bias(faces: &[BoundingBox], height: u32) -> f64 {
    if faces.is_empty() {
        return 0.15; // Default: slight bias toward top
    }

    let h = height as f64;

    // Compute the bounding box that contains all faces
    let min_top = faces.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
    let max_bottom = faces.iter().map(|f| f.y2()).fold(0.0f64, f64::max);

    // Face metrics
    let face_center_y = (min_top + max_bottom) / 2.0;
    let face_height = max_bottom - min_top;

    // Required headroom: 60% of face height above the face top
    // Face detectors return boxes from ~eyebrow to chin, but we need room for
    // the full head/scalp which extends significantly above the detected face.
    // This is especially important for bald individuals.
    let headroom = face_height * 0.60;
    let required_visible_top = (min_top - headroom).max(0.0);

    // Target: place face center at ~38% from top of crop (slightly below rule-of-thirds)
    let target_face_ratio = 0.38;
    let face_y_ratio = face_center_y / h;

    // Base bias: shift crop downward to achieve target positioning
    let aesthetic_bias = (face_y_ratio - target_face_ratio).max(0.0);

    // Safety constraint: crop_y must not exceed required_visible_top
    // Since crop_y = vertical_margin * bias, and vertical_margin ≈ h * 0.2 to 0.5 typically,
    // we compute a safe maximum bias that ensures required_visible_top is always visible.
    //
    // For a typical split panel: crop_height ≈ h * 0.8, so vertical_margin ≈ h * 0.2
    // Safe crop_y = required_visible_top → bias = required_visible_top / vertical_margin
    // Approximate vertical_margin as h * 0.25 for safety calculation
    let approx_margin_ratio = 0.25;
    let max_safe_bias = if h * approx_margin_ratio > 0.0 {
        required_visible_top / (h * approx_margin_ratio)
    } else {
        0.0
    };

    // Use the more conservative (lower) of aesthetic and safe bias
    let bias = aesthetic_bias.min(max_safe_bias);

    // Additional safety: if face is very large (>30% of frame height),
    // be extra conservative to ensure it fits
    let face_height_ratio = face_height / h;
    let bias = if face_height_ratio > 0.3 {
        bias.min(0.15)
    } else {
        bias
    };

    bias.clamp(0.0, 0.5)
}

/// Compute split layout info from raw detections.
///
/// This is used when cached analysis is unavailable and we need to
/// compute split info from fallback detection results.
pub fn compute_split_info_from_detections(
    detections: &[Vec<Detection>],
    width: u32,
    _height: u32,
    _duration: f64,
    fps_sample: f64,
) -> SplitLayoutInfo {
    const MIN_SIMULTANEOUS_SECONDS: f64 = 3.0;

    if detections.is_empty() {
        return SplitLayoutInfo::no_split();
    }

    let sample_interval = 1.0 / fps_sample.max(2.0);
    let center_x = width as f64 / 2.0;

    let mut simultaneous_time = 0.0;
    let mut distinct_tracks = std::collections::HashSet::new();
    let mut left_faces = Vec::new();
    let mut right_faces = Vec::new();

    for frame_dets in detections {
        if frame_dets.len() >= 2 {
            simultaneous_time += sample_interval;
        }
        for det in frame_dets {
            distinct_tracks.insert(det.track_id);
            if det.bbox.cx() < center_x {
                left_faces.push(det.bbox);
            } else {
                right_faces.push(det.bbox);
            }
        }
    }

    let should_split =
        distinct_tracks.len() >= 2 && simultaneous_time >= MIN_SIMULTANEOUS_SECONDS;

    info!(
        "[SPLIT_INFO] {} tracks, {:.1}s simultaneous (need >= {:.1}s) → {}",
        distinct_tracks.len(),
        simultaneous_time,
        MIN_SIMULTANEOUS_SECONDS,
        if should_split { "SPLIT" } else { "FULL-FRAME" }
    );

    SplitLayoutInfo {
        should_split,
        simultaneous_time,
        distinct_tracks: distinct_tracks.len(),
        left_faces,
        right_faces,
    }
}

/// Compute speaker-aware split boxes from cached analysis.
///
/// Returns (left_box, right_box) representing the average face positions
/// for each side, suitable for speaker-aware split rendering.
pub fn compute_speaker_split_boxes(
    analysis: &SceneNeuralAnalysis,
    width: u32,
    height: u32,
) -> Option<(BoundingBox, BoundingBox)> {
    use super::split_evaluator::SplitEvaluator;

    let fw = width as f32;
    let fh = height as f32;

    // Convert to detection format
    let frames: Vec<Vec<Detection>> = analysis
        .frames
        .iter()
        .map(|frame| {
            frame
                .faces
                .iter()
                .map(|face| {
                    let (x, y, w, h) = face.bbox.to_pixels(fw, fh);
                    let bbox = BoundingBox::new(x as f64, y as f64, w as f64, h as f64);
                    Detection::with_mouth(
                        frame.time,
                        bbox,
                        face.score as f64,
                        face.track_id.unwrap_or(0),
                        face.mouth_openness.map(|m| m as f64),
                    )
                })
                .collect()
        })
        .collect();

    let duration = analysis
        .frames
        .last()
        .map(|f| f.time)
        .unwrap_or(0.0)
        .max(1.0);

    SplitEvaluator::evaluate_speaker_split(&frames, width, height, duration)
}

/// Centralized fallback detection when cache is unavailable.
///
/// This function provides a single entry point for running detection
/// when cached analysis is not available. All intelligent processors
/// should use this instead of calling detector methods directly.
///
/// # Arguments
/// * `video_path` - Path to the video segment
/// * `tier` - Detection tier to use
/// * `start_time` - Start time in seconds
/// * `end_time` - End time in seconds
/// * `width` - Video width
/// * `height` - Video height
/// * `fps` - Video frame rate
///
/// # Returns
/// Vector of frame detections
pub async fn run_fallback_detection(
    video_path: &Path,
    tier: DetectionTier,
    start_time: f64,
    end_time: f64,
    width: u32,
    height: u32,
    fps: f64,
) -> MediaResult<Vec<Vec<Detection>>> {
    warn!(
        tier = %tier,
        start = start_time,
        end = end_time,
        "Running fallback detection (cache miss)"
    );

    // Try optimized detector first for all tiers (temporal decimation + Kalman tracking)
    // This provides ~5x throughput improvement with INT8 models
    let config = IntelligentCropConfig::default();

    if config.engine_mode == FaceEngineMode::Optimized {
        info!(
            "[FALLBACK] Using Optimized pipeline (temporal decimation + Kalman) for {:?}",
            tier
        );
        match OptimizedFaceDetector::new(config.clone()) {
            Ok(detector) => {
                return detector
                    .detect_in_video(video_path, start_time, end_time, width, height, fps)
                    .await;
            }
            Err(e) => {
                warn!(
                    "Failed to create OptimizedFaceDetector, falling back to legacy: {}",
                    e
                );
            }
        }
    }

    // Legacy fallback path - only used if optimized detector fails
    match tier {
        DetectionTier::SpeakerAware | DetectionTier::Cinematic => {
            info!("[FALLBACK] Using SpeakerAware pipeline (YuNet + FaceMesh)");
            let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware).build()?;
            let result = pipeline.analyze(video_path, start_time, end_time).await?;
            Ok(result.frames.into_iter().map(|f| f.faces).collect())
        }
        DetectionTier::MotionAware => {
            info!("[FALLBACK] Using MotionAware pipeline (motion heuristics)");
            let pipeline = PipelineBuilder::for_tier(DetectionTier::MotionAware).build()?;
            let result = pipeline.analyze(video_path, start_time, end_time).await?;
            Ok(result.frames.into_iter().map(|f| f.faces).collect())
        }
        DetectionTier::Basic | DetectionTier::None => {
            info!("[FALLBACK] Using Basic pipeline (YuNet face detection)");
            let detector = FaceDetector::new(config);
            detector
                .detect_in_video(video_path, start_time, end_time, width, height, fps)
                .await
        }
    }
}

/// Get detections from cache or run fallback detection.
///
/// This is the primary entry point for all intelligent processors.
/// It handles the cache-or-compute pattern consistently.
///
/// # Arguments
/// * `cached_analysis` - Optional cached neural analysis
/// * `video_path` - Path to the video segment (for fallback)
/// * `tier` - Detection tier to use
/// * `start_time` - Start time in seconds
/// * `end_time` - End time in seconds
/// * `width` - Video width
/// * `height` - Video height
/// * `fps` - Video frame rate
pub async fn get_detections(
    cached_analysis: Option<&SceneNeuralAnalysis>,
    video_path: &Path,
    tier: DetectionTier,
    start_time: f64,
    end_time: f64,
    width: u32,
    height: u32,
    fps: f64,
) -> MediaResult<Vec<Vec<Detection>>> {
    if let Some(analysis) = cached_analysis {
        info!(
            frames = analysis.frames.len(),
            tier = %analysis.detection_tier,
            "Using cached neural analysis (SKIPPING detection)"
        );
        Ok(convert_cached_analysis(analysis, width, height))
    } else {
        run_fallback_detection(video_path, tier, start_time, end_time, width, height, fps).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::{FaceDetection, FrameAnalysis};

    fn create_test_analysis() -> SceneNeuralAnalysis {
        let mut analysis = SceneNeuralAnalysis::new("test_video", 1);

        // Frame with two faces - one left, one right
        let mut frame = FrameAnalysis::new(0.0);
        frame.add_face(
            FaceDetection::new(
                vclip_models::BoundingBox::from_pixels(100.0, 200.0, 150.0, 150.0, 1920.0, 1080.0),
                0.95,
            )
            .with_track_id(1),
        );
        frame.add_face(
            FaceDetection::new(
                vclip_models::BoundingBox::from_pixels(
                    1600.0, 200.0, 150.0, 150.0, 1920.0, 1080.0,
                ),
                0.92,
            )
            .with_track_id(2),
        );
        analysis.add_frame(frame);

        analysis
    }

    #[test]
    fn test_convert_cached_analysis() {
        let analysis = create_test_analysis();
        let detections = convert_cached_analysis(&analysis, 1920, 1080);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].len(), 2);
    }

    #[test]
    fn test_extract_split_info() {
        let mut analysis = create_test_analysis();

        // Add more frames to meet simultaneous time threshold
        for i in 1..10 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.5);
            frame.add_face(
                FaceDetection::new(
                    vclip_models::BoundingBox::from_pixels(
                        100.0, 200.0, 150.0, 150.0, 1920.0, 1080.0,
                    ),
                    0.95,
                )
                .with_track_id(1),
            );
            frame.add_face(
                FaceDetection::new(
                    vclip_models::BoundingBox::from_pixels(
                        1600.0, 200.0, 150.0, 150.0, 1920.0, 1080.0,
                    ),
                    0.92,
                )
                .with_track_id(2),
            );
            analysis.add_frame(frame);
        }

        let info = extract_split_info(&analysis, 1920, 1080, 5.0);

        assert!(info.should_split);
        assert_eq!(info.distinct_tracks, 2);
        assert!(!info.left_faces.is_empty());
        assert!(!info.right_faces.is_empty());
    }

    #[test]
    fn test_vertical_bias_computation() {
        let faces = vec![BoundingBox::new(100.0, 200.0, 150.0, 150.0)];
        let bias = compute_vertical_bias(&faces, 1080);
        assert!(bias >= 0.0 && bias <= 0.4);
    }
}
