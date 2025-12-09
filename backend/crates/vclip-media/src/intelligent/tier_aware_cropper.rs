//! Tier-aware intelligent cropping pipeline.
//!
//! This module provides the main entry point for tier-specific intelligent cropping.
//! It orchestrates face detection, speaker detection, activity analysis, and camera
//! planning based on the detection tier.
//!
//! # Tier Behavior
//!
//! - **Basic**: Face detection → Camera smoothing → Crop planning
//! - **SpeakerAware**: Face detection + Speaker + Activity → Activity-aware smoothing

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::crop_planner::CropPlanner;
use super::detector::FaceDetector;
use super::motion::MotionDetector;
use crate::detection::pipeline_builder::PipelineBuilder;
use super::models::AspectRatio;
use super::renderer::IntelligentRenderer;
use super::tier_aware_smoother::TierAwareCameraSmoother;
use crate::clip::extract_segment;
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Tier-aware intelligent cropper.
///
/// Orchestrates the full intelligent cropping pipeline with tier-specific behavior.
pub struct TierAwareIntelligentCropper {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    detector: FaceDetector,
}

impl TierAwareIntelligentCropper {
    /// Create a new tier-aware cropper.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self {
            detector: FaceDetector::new(config.clone()),
            config,
            tier,
        }
    }

    /// Create with default configuration.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self::new(IntelligentCropConfig::default(), tier)
    }

    /// Get the detection tier.
    pub fn tier(&self) -> DetectionTier {
        self.tier
    }

    /// Process a pre-cut video segment with tier-aware intelligent cropping.
    ///
    /// # Arguments
    /// * `input` - Path to the pre-cut video segment
    /// * `output` - Path for the output file
    ///
    /// # Returns
    /// Ok(()) on success
    pub async fn process<P: AsRef<Path>>(&self, input: P, output: P) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        info!(
            "Starting tier-aware intelligent crop (tier: {:?}) for {:?}",
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

        // 2. Detect faces (SpeakerAware uses face mesh pipeline, MotionAware uses motion heuristic)
        info!("Step 1/4: Detecting faces...");
        let (detections, _speaker_segments) = match self.tier {
            DetectionTier::SpeakerAware => {
                let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware).build()?;
                let res = pipeline.analyze(input, start_time, end_time).await?;
                let frames: Vec<_> = res.frames.iter().map(|f| f.faces.clone()).collect();
                let segments = res.speaker_segments.unwrap_or_default();
                (frames, segments)
            }
            DetectionTier::MotionAware => {
                let motion_frames = Self::detect_motion_tracks(
                    input,
                    start_time,
                    end_time,
                    width,
                    height,
                    fps,
                    self.config.fps_sample,
                )?;
                (motion_frames, Vec::new())
            }
            _ => {
                let detections = self
                    .detector
                    .detect_in_video(input, start_time, end_time, width, height, fps)
                    .await?;

                let total_detections: usize = detections.iter().map(|d| d.len()).sum();
                info!("  Found {} face detections", total_detections);

                (detections, Vec::new())
            }
        };

        let total_detections: usize = detections.iter().map(|d| d.len()).sum();
        info!("  Found {} face detections", total_detections);

        // 4. Compute camera plan with tier-aware smoother
        info!("Step 3/4: Computing tier-aware camera path...");
        let mut smoother = TierAwareCameraSmoother::new(self.config.clone(), self.tier, fps);

        let camera_keyframes = smoother.compute_camera_plan(
            &detections,
            width,
            height,
            start_time,
            end_time,
        );
        info!("  Generated {} camera keyframes", camera_keyframes.len());

        // 5. Compute crop windows
        info!("Step 4/4: Computing crop windows...");
        let planner = CropPlanner::new(self.config.clone(), width, height);
        let target_aspect = AspectRatio::new(9, 16); // Portrait 9:16
        let crop_windows = planner.compute_crop_windows(&camera_keyframes, &target_aspect);
        info!("  Generated {} crop windows", crop_windows.len());

        // 6. Render the output
        info!("Rendering output...");
        let renderer = IntelligentRenderer::new(self.config.clone());
        renderer
            .render(input, output, &crop_windows, start_time, duration)
            .await?;

        info!("Tier-aware intelligent crop complete: {:?}", output);

        // Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("Failed to generate thumbnail: {}", e);
        }

        Ok(())
    }

    /// Detect motion centers for MotionAware tier, producing synthetic detections.
    fn detect_motion_tracks(
        segment: &Path,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        _fps: f64,
        sample_rate: f64,
    ) -> MediaResult<Vec<Vec<super::models::Detection>>> {
        use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
        use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_MSEC};

        let mut cap = VideoCapture::from_file(segment.to_str().unwrap_or(""), CAP_ANY)
            .map_err(|e| crate::error::MediaError::detection_failed(format!("Open video: {e}")))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(crate::error::MediaError::detection_failed(
                "Failed to open video for motion analysis",
            ));
        }

        let mut detector = MotionDetector::new(width as i32, height as i32);
        let sample_interval = 1.0 / sample_rate.max(1e-3);
        let mut frames = Vec::new();
        let mut current_time = start_time;
        let mut last_detection: Option<super::models::Detection> = None;
        let mut last_seen_time: Option<f64> = None;
        const DECAY_SECONDS: f64 = 2.0;

        while current_time < end_time {
            cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Seek: {e}")))?;

            let mut frame = opencv::core::Mat::default();
            if !cap
                .read(&mut frame)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))? || frame.empty() {
                frames.push(Vec::new());
                current_time += sample_interval;
                continue;
            }

            let detection = detector
                .detect_center(&frame)?
                .map(|center| {
                    // Use a moderate box size around the motion center.
                    let size = (width.min(height) as f64 * 0.35).max(64.0);
                    let bbox = super::models::BoundingBox::new(
                        center.x as f64 - size / 2.0,
                        center.y as f64 - size / 2.0,
                        size,
                        size,
                    )
                    .clamp(width, height);

                    // Single synthetic track id
                    super::models::Detection::new(current_time, bbox, 1.0, 1)
                });

            // Coasting: hold last valid motion target for a decay window.
            let frame_dets = if let Some(det) = detection {
                last_seen_time = Some(current_time);
                last_detection = Some(det.clone());
                vec![det]
            } else if let (Some(last_det), Some(last_time)) = (&last_detection, last_seen_time) {
                if current_time - last_time <= DECAY_SECONDS {
                    let mut held = last_det.clone();
                    held.time = current_time;
                    vec![held]
                } else {
                    last_detection = None;
                    last_seen_time = None;
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            frames.push(frame_dets);
            current_time += sample_interval;
        }

        Ok(frames)
    }
}

/// Create a tier-aware intelligent clip from a video file.
///
/// This is the main entry point for tier-specific intelligent styles.
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `tier` - Detection tier controlling which providers are used
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_tier_aware_intelligent_clip<P, F>(
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
        "Extracting segment for tier-aware intelligent crop: {:.2}s - {:.2}s (tier: {:?})",
        start_secs, end_secs, tier
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Apply tier-aware intelligent cropping
    let config = IntelligentCropConfig::default();
    let cropper = TierAwareIntelligentCropper::new(config, tier);
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
        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::Basic);
        assert_eq!(cropper.tier(), DetectionTier::Basic);

        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::SpeakerAware);
        assert_eq!(cropper.tier(), DetectionTier::SpeakerAware);
    }

    #[test]
    fn test_tier_uses_audio() {
        assert!(!DetectionTier::None.uses_audio());
        assert!(!DetectionTier::Basic.uses_audio());
        assert!(!DetectionTier::SpeakerAware.uses_audio());
    }
}
