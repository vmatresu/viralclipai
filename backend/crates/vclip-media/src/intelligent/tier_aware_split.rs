//! Tier-aware split video processing.
//!
//! This module extends the split view processing with tier-specific behavior:
//! - **Basic**: Fixed vertical positioning (current behavior)
//! - **AudioAware**: Speaker-aware panel highlighting (future)
//! - **SpeakerAware**: Dynamic per-panel positioning based on face detection
//!
//! For split view styles, the tier primarily affects:
//! 1. Per-panel vertical positioning based on detected face positions
//! 2. Logging and metrics for tier-specific processing

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::detector::FaceDetector;
use super::models::BoundingBox;
use super::speaker_detector::SpeakerDetector;
use crate::clip::extract_segment;
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Tier-aware split processor.
pub struct TierAwareSplitProcessor {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    detector: FaceDetector,
    #[allow(dead_code)]
    speaker_detector: SpeakerDetector,
}

impl TierAwareSplitProcessor {
    /// Create a new tier-aware split processor.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self {
            detector: FaceDetector::new(config.clone()),
            speaker_detector: SpeakerDetector::new(),
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

    /// Process a video segment with tier-aware split.
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();

        info!(
            "Tier-aware split processing (tier: {:?}): {:?}",
            self.tier, segment
        );

        // 1. Get video metadata
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let duration = video_info.duration;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, video_info.fps, duration
        );

        // 2. Detect faces for positioning (AudioAware and SpeakerAware tiers)
        let (left_vertical_bias, right_vertical_bias) = if self.tier.requires_yunet() {
            self.compute_face_aware_positioning(segment, width, height, duration).await
        } else {
            // Basic/None tier: use fixed positioning
            (0.0, 0.15)
        };

        info!(
            "Vertical positioning: left={:.2}, right={:.2}",
            left_vertical_bias, right_vertical_bias
        );

        // 3. Process with computed positioning
        self.process_split_view(
            segment,
            output,
            width,
            height,
            left_vertical_bias,
            right_vertical_bias,
            encoding,
        )
        .await?;

        // 4. Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("Failed to generate thumbnail: {}", e);
        }

        info!("Tier-aware split complete: {:?}", output);
        Ok(())
    }

    /// Compute face-aware vertical positioning for each panel.
    ///
    /// Returns (left_bias, right_bias) where 0.0 = top, 1.0 = bottom.
    async fn compute_face_aware_positioning<P: AsRef<Path>>(
        &self,
        segment: P,
        width: u32,
        height: u32,
        duration: f64,
    ) -> (f64, f64) {
        let segment = segment.as_ref();

        // Sample a few frames to detect face positions
        let sample_duration = duration.min(5.0); // Sample first 5 seconds
        let fps = self.config.fps_sample;

        match self.detector.detect_in_video(
            segment,
            0.0,
            sample_duration,
            width,
            height,
            fps,
        ).await {
            Ok(detections) => {
                // Analyze face positions in left and right halves
                let center_x = width as f64 / 2.0;
                let mut left_faces: Vec<&BoundingBox> = Vec::new();
                let mut right_faces: Vec<&BoundingBox> = Vec::new();

                for frame_dets in &detections {
                    for det in frame_dets {
                        if det.bbox.cx() < center_x {
                            left_faces.push(&det.bbox);
                        } else {
                            right_faces.push(&det.bbox);
                        }
                    }
                }

                // Compute average vertical position for each side
                let left_bias = self.compute_vertical_bias(&left_faces, height);
                let right_bias = self.compute_vertical_bias(&right_faces, height);

                info!(
                    "Face detection: {} left faces, {} right faces",
                    left_faces.len(),
                    right_faces.len()
                );

                (left_bias, right_bias)
            }
            Err(e) => {
                tracing::warn!("Face detection failed, using defaults: {}", e);
                (0.0, 0.15)
            }
        }
    }

    /// Compute vertical bias from detected faces.
    ///
    /// Returns a value from 0.0 (top) to 1.0 (bottom) indicating where
    /// to position the crop to best capture the faces.
    fn compute_vertical_bias(&self, faces: &[&BoundingBox], height: u32) -> f64 {
        if faces.is_empty() {
            return 0.15; // Default: slight bias toward top
        }

        // Compute average face center Y position
        let avg_cy: f64 = faces.iter().map(|f| f.cy()).sum::<f64>() / faces.len() as f64;

        // Convert to bias (0.0 = face at top, 1.0 = face at bottom)
        let normalized_y = avg_cy / height as f64;

        // We want to position the crop so the face is in the upper portion
        // If face is at 30% of frame height, we want bias ~0.0 (crop from top)
        // If face is at 50% of frame height, we want bias ~0.15
        // If face is at 70% of frame height, we want bias ~0.3

        // Clamp to reasonable range
        let bias = (normalized_y - 0.3).max(0.0).min(0.4);

        bias
    }

    /// Process the video with split view using computed positioning.
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        left_vertical_bias: f64,
        right_vertical_bias: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let temp_dir = tempfile::tempdir()?;

        // Calculate crop dimensions (45% from each side)
        let crop_fraction = 0.45;
        let crop_width = (width as f64 * crop_fraction).round() as u32;
        let right_start_x = width - crop_width;

        // Calculate 9:8 tile dimensions
        let tile_height = ((crop_width as f64 * 8.0 / 9.0).round() as u32).min(height);
        let vertical_margin = height.saturating_sub(tile_height);

        // Apply computed vertical biases
        let left_crop_y = (vertical_margin as f64 * left_vertical_bias).round() as u32;
        let right_crop_y = (vertical_margin as f64 * right_vertical_bias).round() as u32;

        // Step 1: Extract left and right portions
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        info!(
            "  Extracting left person (0 to {}px, y_offset={})",
            crop_width, left_crop_y
        );

        let left_filter = format!(
            "crop={}:{}:0:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width, height,
            crop_width, tile_height, left_crop_y
        );

        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter(&left_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_left).await?;

        info!(
            "  Extracting right person ({}px to {}px, y_offset={})",
            right_start_x, width, right_crop_y
        );

        let right_filter = format!(
            "crop={}:{}:{}:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width, height, right_start_x,
            crop_width, tile_height, right_crop_y
        );

        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter(&right_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_right).await?;

        // Step 2: Stack the halves vertically
        info!("  Stacking panels...");
        let stack_crf = encoding.crf.saturating_add(2);

        let stack_args = vec![
            "-y".to_string(),
            "-i".to_string(),
            left_half.to_string_lossy().to_string(),
            "-i".to_string(),
            right_half.to_string_lossy().to_string(),
            "-filter_complex".to_string(),
            "[0:v][1:v]vstack=inputs=2".to_string(),
            "-c:v".to_string(),
            encoding.codec.clone(),
            "-preset".to_string(),
            encoding.preset.clone(),
            "-crf".to_string(),
            stack_crf.to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            encoding.audio_bitrate.clone(),
            "-movflags".to_string(),
            "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        let stack_status = tokio::process::Command::new("ffmpeg")
            .args(&stack_args)
            .output()
            .await?;

        if !stack_status.status.success() {
            return Err(MediaError::ffmpeg_failed(
                "Stacking failed",
                Some(String::from_utf8_lossy(&stack_status.stderr).to_string()),
                stack_status.status.code(),
            ));
        }

        Ok(())
    }
}

/// Create a tier-aware intelligent split clip from a video file.
pub async fn create_tier_aware_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
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

    // Step 1: Extract segment
    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for tier-aware split: {:.2}s - {:.2}s (tier: {:?})",
        start_secs, end_secs, tier
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with tier-aware split
    let config = IntelligentCropConfig::default();
    let processor = TierAwareSplitProcessor::new(config, tier);
    let result = processor.process(segment_path.as_path(), output, encoding).await;

    // Step 3: Cleanup
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
    fn test_processor_creation() {
        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::Basic);
        assert_eq!(processor.tier(), DetectionTier::Basic);

        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::AudioAware);
        assert_eq!(processor.tier(), DetectionTier::AudioAware);
    }

    #[test]
    fn test_vertical_bias_computation() {
        let config = IntelligentCropConfig::default();
        let processor = TierAwareSplitProcessor::new(config, DetectionTier::Basic);

        // Face at top of frame -> low bias
        let top_face = BoundingBox::new(100.0, 50.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&top_face], 1080);
        assert!(bias < 0.1, "Top face should have low bias: {}", bias);

        // Face at middle of frame -> medium bias
        let mid_face = BoundingBox::new(100.0, 440.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&mid_face], 1080);
        assert!(bias > 0.1 && bias < 0.3, "Mid face should have medium bias: {}", bias);

        // Face at bottom of frame -> higher bias (clamped)
        let bottom_face = BoundingBox::new(100.0, 800.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&bottom_face], 1080);
        assert!(bias >= 0.3, "Bottom face should have higher bias: {}", bias);
    }

    #[test]
    fn test_empty_faces_default_bias() {
        let config = IntelligentCropConfig::default();
        let processor = TierAwareSplitProcessor::new(config, DetectionTier::Basic);

        let bias = processor.compute_vertical_bias(&[], 1080);
        assert!((bias - 0.15).abs() < 0.01, "Empty faces should use default bias");
    }
}
