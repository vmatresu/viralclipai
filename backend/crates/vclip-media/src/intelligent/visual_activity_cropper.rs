//! Visual activity-based intelligent cropping pipeline.
//!
//! This module provides the processing pipeline for visual-based intelligent styles:
//! - `IntelligentMotion` - uses frame differencing to detect active faces
//!
//! Fully visual heuristics; no audio inputs are used.
//!
//! # Pipeline
//!
//! 1. Extract video segment
//! 2. Detect faces using YuNet
//! 3. Compute visual activity scores (motion, size changes)
//! 4. Select active face using activity scoring
//! 5. Compute camera path with tier-aware smoothing
//! 6. Render output with intelligent cropping

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::crop_planner::CropPlanner;
use super::detector::FaceDetector;
use super::models::AspectRatio;
use super::renderer::IntelligentRenderer;
use super::tier_aware_smoother::TierAwareCameraSmoother;
use crate::clip::extract_segment;
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Visual activity cropper for motion-based intelligent styles.
///
/// Uses visual cues (motion, size changes) instead of stereo audio analysis.
/// Works with any audio format.
pub struct VisualActivityCropper {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    detector: FaceDetector,
}

impl VisualActivityCropper {
    /// Create a new visual activity cropper.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self {
            detector: FaceDetector::new(config.clone()),
            config,
            tier,
        }
    }

    /// Create with default configuration for MotionAware tier.
    pub fn motion_aware() -> Self {
        Self::new(IntelligentCropConfig::default(), DetectionTier::MotionAware)
    }

    /// Get the detection tier.
    pub fn tier(&self) -> DetectionTier {
        self.tier
    }

    /// Process a pre-cut video segment with visual activity-based intelligent cropping.
    ///
    /// # Arguments
    /// * `input` - Path to the pre-cut video segment
    /// * `output` - Path for the output file
    pub async fn process<P: AsRef<Path>>(&self, input: P, output: P) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        info!(
            "Starting visual activity crop (tier: {:?}) for {:?}",
            self.tier, input
        );

        // 1. Get video metadata
        let video_info = probe_video(input).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, fps, duration
        );

        let start_time = 0.0;
        let end_time = duration;

        // 2. Detect faces using YuNet
        info!("Step 1/3: Detecting faces...");
        let detections = self
            .detector
            .detect_in_video(input, start_time, end_time, width, height, fps)
            .await?;

        let total_detections: usize = detections.iter().map(|d| d.len()).sum();
        info!("  Found {} face detections", total_detections);

        // 3. Compute camera plan with visual activity-aware smoother
        // For visual activity tiers, we skip audio-based speaker detection entirely
        // and rely on face confidence, size, and position for prioritization
        info!("Step 2/3: Computing visual activity-aware camera path...");
        let mut smoother = TierAwareCameraSmoother::new(self.config.clone(), self.tier, fps);
        // No speaker segments - visual activity tiers don't use stereo audio

        let camera_keyframes = smoother.compute_camera_plan(
            &detections,
            width,
            height,
            start_time,
            end_time,
        );
        info!("  Generated {} camera keyframes", camera_keyframes.len());

        // 4. Compute crop windows
        info!("Step 3/3: Computing crop windows...");
        let planner = CropPlanner::new(self.config.clone(), width, height);
        let target_aspect = AspectRatio::new(9, 16); // Portrait 9:16
        let crop_windows = planner.compute_crop_windows(&camera_keyframes, &target_aspect);
        info!("  Generated {} crop windows", crop_windows.len());

        // 5. Render the output
        info!("Rendering output...");
        let renderer = IntelligentRenderer::new(self.config.clone());
        renderer
            .render(input, output, &crop_windows, start_time, duration)
            .await?;

        info!("Visual activity crop complete: {:?}", output);

        // Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("Failed to generate thumbnail: {}", e);
        }

        Ok(())
    }
}

/// Create a visual activity intelligent clip from a video file.
///
/// This is the main entry point for Motion-aware intelligent styles.
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `tier` - Detection tier (MotionAware)
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_visual_activity_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    _encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    // Step 1: Extract segment to temporary file
    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for visual activity crop: {:.2}s - {:.2}s (tier: {:?})",
        start_secs, end_secs, tier
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Apply visual activity cropping
    let config = IntelligentCropConfig::default();
    let cropper = VisualActivityCropper::new(config, tier);
    let result = cropper.process(segment_path.as_path(), output).await;

    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!(
                "Failed to delete temporary segment file {}: {}",
                segment_path.display(),
                e
            );
        } else {
            info!("Deleted temporary segment: {}", segment_path.display());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cropper_creation() {
        let cropper = VisualActivityCropper::motion_aware();
        assert_eq!(cropper.tier(), DetectionTier::MotionAware);
    }

    #[test]
    fn test_tier_uses_visual_activity() {
        assert!(DetectionTier::MotionAware.uses_visual_activity());
        assert!(DetectionTier::SpeakerAware.uses_visual_activity());
    }

    #[test]
    fn test_visual_tiers_dont_use_audio() {
        assert!(!DetectionTier::MotionAware.uses_audio());
    }
}
