//! Tier-aware split video processing.
//!
//! This module extends the split view processing with tier-specific behavior:
//! - **Basic**: Fixed vertical positioning (current behavior)
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
use crate::detection::pipeline_builder::PipelineBuilder;
use crate::intelligent::Detection;
use crate::clip::extract_segment;
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use crate::intelligent::stacking::stack_halves;

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

        // Speaker-aware split uses dedicated mouth-openness path.
        if self.tier == DetectionTier::SpeakerAware {
            if let Err(e) =
                self.process_speaker_aware_split(segment, output, width, height, duration, encoding).await
            {
                tracing::warn!("Speaker-aware split failed, falling back to default split: {}", e);
            } else {
                return Ok(());
            }
        }

        // 2. Detect faces for positioning (SpeakerAware tiers)
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

    /// Speaker-aware split path with mouth-open activity and robust left/right mapping.
    async fn process_speaker_aware_split(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        duration: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let center_x = width as f64 / 2.0;
        let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware).build()?;
        let result = pipeline.analyze(segment, 0.0, duration).await?;
        if result.frames.is_empty() {
            return Err(crate::error::MediaError::detection_failed(
                "Speaker-aware pipeline returned no frames",
            ));
        }
        let frames: Vec<Vec<Detection>> = result.frames.iter().map(|f| f.faces.clone()).collect();
        let split_eval = Self::evaluate_speaker_split(&frames, width, height, duration);

        if split_eval.is_none() {
            tracing::info!(
                "Speaker-aware split: not enough dual activity -> single view"
            );
            let cropper = super::tier_aware_cropper::TierAwareIntelligentCropper::new(
                self.config.clone(),
                DetectionTier::SpeakerAware,
            );
            return cropper.process(segment, output).await;
        }

        let (left_box, right_box) = split_eval.unwrap();

        // Width tuned to keep single speaker per panel
        let crop_width_left = left_box.width.min(width as f64 * 0.55).max(width as f64 * 0.25);
        let crop_width_right = right_box.width.min(width as f64 * 0.55).max(width as f64 * 0.25);

        let left_cx = left_box.cx();
        let right_cx = right_box.cx();

        let left_crop_x = (left_cx - crop_width_left / 2.0)
            .max(0.0)
            .min(center_x - crop_width_left * 0.1);
        let right_crop_x = (right_cx - crop_width_right / 2.0)
            .max(center_x)
            .min(width as f64 - crop_width_right);

        let tile_height_left = (crop_width_left * 8.0 / 9.0).min(height as f64);
        let tile_height_right = (crop_width_right * 8.0 / 9.0).min(height as f64);

        let vertical_margin_left = height as f64 - tile_height_left;
        let vertical_margin_right = height as f64 - tile_height_right;

        let left_bias = (left_box.cy() / height as f64 - 0.3).max(0.0).min(0.4);
        let right_bias = (right_box.cy() / height as f64 - 0.3).max(0.0).min(0.4);

        let left_crop_y = (vertical_margin_left * left_bias).round();
        let right_crop_y = (vertical_margin_right * right_bias).round();

        let temp_dir = tempfile::tempdir()?;
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        let left_filter = format!(
            "crop={}:{}:{}:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width_left.round(),
            height,
            left_crop_x.round(),
            crop_width_left.round(),
            tile_height_left.round(),
            left_crop_y,
        );

        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter(&left_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);
        FfmpegRunner::new().run(&cmd_left).await?;

        let right_filter = format!(
            "crop={}:{}:{}:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width_right.round(),
            height,
            right_crop_x.round(),
            crop_width_right.round(),
            tile_height_right.round(),
            right_crop_y,
        );

        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter(&right_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);
        FfmpegRunner::new().run(&cmd_right).await?;

        info!("  Stacking panels (left→top, right→bottom)...");
        stack_halves(&left_half, &right_half, output, encoding).await
    }

    /// Evaluate whether we should enter split mode and return per-side boxes.
    fn evaluate_speaker_split(
        frames: &[Vec<Detection>],
        width: u32,
        height: u32,
        duration: f64,
    ) -> Option<(BoundingBox, BoundingBox)> {
        // MAR thresholds tuned for normal speech (mouth height ≈ 10–20% of width)
        const TALK_ON: f64 = 0.15;
        const TALK_OFF: f64 = 0.05;
        const MOUTH_ALPHA: f64 = 0.6;
        const MIN_SPLIT_TIME: f64 = 0.3;
        const MARGIN: f64 = 0.25;

        if frames.is_empty() {
            return None;
        }

        let sample_interval = if frames.len() > 1 {
            duration / frames.len() as f64
        } else {
            1.0 / 8.0
        };
        let center_x = width as f64 / 2.0;

        use std::collections::HashMap;
        let mut mouth_ema: HashMap<u32, f64> = HashMap::new();
        let mut track_side: HashMap<u32, bool> = HashMap::new(); // true=left
        let mut talk_state: HashMap<u32, bool> = HashMap::new();

        let mut left_boxes: Vec<BoundingBox> = Vec::new();
        let mut right_boxes: Vec<BoundingBox> = Vec::new();
        let mut split_active_time = 0.0;

        for frame in frames {
            for det in frame {
                track_side
                    .entry(det.track_id)
                    .or_insert_with(|| det.bbox.cx() < center_x);
                let m = det.mouth_openness.unwrap_or(0.0).clamp(0.0, 2.0);
                let ema = mouth_ema.entry(det.track_id).or_insert(m);
                *ema = MOUTH_ALPHA * m + (1.0 - MOUTH_ALPHA) * *ema;

                let entry = talk_state.entry(det.track_id).or_insert(false);
                *entry = apply_hysteresis(*entry, *ema, TALK_ON, TALK_OFF);

                if *track_side.get(&det.track_id).unwrap_or(&true) {
                    left_boxes.push(det.bbox);
                } else {
                    right_boxes.push(det.bbox);
                }
            }

            let mut left_talking = false;
            let mut right_talking = false;
            for det in frame {
                let side_left = *track_side.get(&det.track_id).unwrap_or(&true);
                let talking = *talk_state.get(&det.track_id).unwrap_or(&false);
                if talking {
                    if side_left {
                        left_talking = true;
                    } else {
                        right_talking = true;
                    }
                }
            }

            if left_talking && right_talking {
                split_active_time += sample_interval;
            } else if split_active_time > 0.0 {
                split_active_time = (split_active_time - sample_interval * 0.5).max(0.0);
            }
        }

        let has_two_tracks = frames.iter().any(|f| f.len() >= 2);
        let should_split = has_two_tracks && split_active_time >= MIN_SPLIT_TIME;

        if !should_split {
            return None;
        }

        let left_union = BoundingBox::union(&left_boxes)
            .unwrap_or_else(|| BoundingBox::new(0.0, 0.0, width as f64 / 2.0, height as f64 * 0.8));
        let right_union = BoundingBox::union(&right_boxes)
            .unwrap_or_else(|| BoundingBox::new(width as f64 / 2.0, 0.0, width as f64 / 2.0, height as f64 * 0.8));

        let expand = |b: BoundingBox| {
            let pad = (b.width.max(b.height)) * MARGIN;
            b.pad(pad)
        };
        let left_box = expand(left_union).clamp(width, height);
        let right_box = expand(right_union).clamp(width, height);

        Some((left_box, right_box))
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
        stack_halves(&left_half, &right_half, output, encoding).await
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

/// Hysteresis helper for talking state.
#[inline]
fn apply_hysteresis(current: bool, ema: f64, on: f64, off: f64) -> bool {
    if current {
        ema >= off
    } else {
        ema >= on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_creation() {
        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::Basic);
        assert_eq!(processor.tier(), DetectionTier::Basic);
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

    #[test]
    fn test_hysteresis_turns_on_and_off() {
        // Off stays off below ON
        assert!(!super::super::super::intelligent::tier_aware_split::apply_hysteresis(false, 0.04, 0.15, 0.05));
        // Turns on when above ON
        assert!(super::super::super::intelligent::tier_aware_split::apply_hysteresis(false, 0.16, 0.15, 0.05));
        // Stays on while above OFF
        assert!(super::super::super::intelligent::tier_aware_split::apply_hysteresis(true, 0.06, 0.15, 0.05));
        // Turns off when below OFF
        assert!(!super::super::super::intelligent::tier_aware_split::apply_hysteresis(true, 0.01, 0.15, 0.05));
    }

    #[test]
    fn test_hysteresis_turns_on_and_off() {
        // Off stays off below ON
        assert!(!apply_hysteresis(false, 0.04, 0.15, 0.05));
        // Turns on when above ON
        assert!(apply_hysteresis(false, 0.16, 0.15, 0.05));
        // Stays on while above OFF
        assert!(apply_hysteresis(true, 0.06, 0.15, 0.05));
        // Turns off when below OFF
        assert!(!apply_hysteresis(true, 0.01, 0.15, 0.05));
    }

    #[test]
    fn test_evaluate_speaker_split_two_speakers_triggers_split() {
        let width = 1920;
        let height = 1080;
        let frames = vec![vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(200.0, 200.0, 200.0, 200.0),
                0.9,
                1,
                Some(0.8),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1400.0, 220.0, 200.0, 200.0),
                0.9,
                2,
                Some(0.8),
            ),
        ]];

        let res = TierAwareSplitProcessor::evaluate_speaker_split(&frames, width, height, 0.5);
        assert!(res.is_some(), "Should split when both are talking");
        let (left_box, right_box) = res.unwrap();
        assert!(left_box.cx() < right_box.cx());
    }

    #[test]
    fn test_evaluate_speaker_split_not_enough_activity() {
        let width = 1920;
        let height = 1080;
        let frames = vec![vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(200.0, 200.0, 200.0, 200.0),
                0.9,
                1,
                Some(0.1),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1400.0, 220.0, 200.0, 200.0),
                0.9,
                2,
                Some(0.1),
            ),
        ]];

        let res = TierAwareSplitProcessor::evaluate_speaker_split(&frames, width, height, 0.5);
        assert!(res.is_none(), "Should stay single when mouths are closed");
    }
}
