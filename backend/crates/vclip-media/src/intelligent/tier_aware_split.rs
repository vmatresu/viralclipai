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
use super::motion::MotionDetector;
use super::models::BoundingBox;
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
}

impl TierAwareSplitProcessor {
    /// Create a new tier-aware split processor.
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

        // 2. Positioning per tier
        let (left_vertical_bias, right_vertical_bias) = match self.tier {
            DetectionTier::MotionAware => {
                self.compute_motion_positioning(segment, width, height, duration)?
            }
            tier if tier.requires_yunet() => {
                self.compute_face_aware_positioning(segment, width, height, duration).await
            }
            _ => {
                // Basic/None tier: use fixed positioning
                (0.0, 0.15)
            }
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

    /// Compute motion-aware vertical positioning for each panel (NN-free).
    ///
    /// Uses dual MotionDetector instances on left/right halves and returns
    /// (top_bias, bottom_bias) where 0.0 = top, 1.0 = bottom.
    fn compute_motion_positioning<P: AsRef<Path>>(
        &self,
        segment: P,
        width: u32,
        height: u32,
        duration: f64,
    ) -> MediaResult<(f64, f64)> {
        use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
        use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_MSEC};

        let segment = segment.as_ref();
        let half_width = (width / 2) as i32;
        let height_i = height as i32;

        let mut cap = VideoCapture::from_file(segment.to_str().unwrap_or(""), CAP_ANY)
            .map_err(|e| crate::error::MediaError::detection_failed(format!("Open video: {e}")))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(crate::error::MediaError::detection_failed(
                "Failed to open video for motion analysis",
            ));
        }

        let mut left_motion = MotionDetector::new(half_width, height_i);
        let mut right_motion = MotionDetector::new(half_width, height_i);

        let sample_interval = 1.0 / self.config.fps_sample.max(1e-3);
        let mut current_time = 0.0;
        let mut left_biases = Vec::new();
        let mut right_biases = Vec::new();

        // Coasting: hold last valid motion target briefly to reduce jitter.
        const DECAY_SECONDS: f64 = 2.0;
        let mut last_left: Option<(f64, f64)> = None; // (time, bias)
        let mut last_right: Option<(f64, f64)> = None;

        while current_time < duration {
            cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Seek: {e}")))?;

            let mut frame = opencv::core::Mat::default();
            if !cap
                .read(&mut frame)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))? || frame.empty()
            {
                current_time += sample_interval;
                continue;
            }

            // Split frame into left/right halves
            let left_roi = opencv::core::Rect::new(0, 0, half_width, height_i);
            let right_roi = opencv::core::Rect::new(half_width, 0, half_width, height_i);

            let mut left_bias_opt = None;
            if let Ok(roi) = opencv::core::Mat::roi(&frame, left_roi) {
                let mut roi_mat = opencv::core::Mat::default();
                if roi.copy_to(&mut roi_mat).is_ok() {
                    if let Ok(center_opt) = left_motion.detect_center(&roi_mat) {
                        if let Some(center) = center_opt {
                            left_bias_opt = Some((center.y as f64 / height as f64).clamp(0.0, 1.0));
                        }
                    }
                }
            }

            let mut right_bias_opt = None;
            if let Ok(roi) = opencv::core::Mat::roi(&frame, right_roi) {
                let mut roi_mat = opencv::core::Mat::default();
                if roi.copy_to(&mut roi_mat).is_ok() {
                    if let Ok(center_opt) = right_motion.detect_center(&roi_mat) {
                        if let Some(center) = center_opt {
                            right_bias_opt = Some((center.y as f64 / height as f64).clamp(0.0, 1.0));
                        }
                    }
                }
            }

            // Coasting logic
            let now = current_time;
            if let Some(b) = left_bias_opt {
                last_left = Some((now, b));
                left_biases.push(b);
            } else if let Some((t, b)) = last_left {
                if now - t <= DECAY_SECONDS {
                    left_biases.push(b);
                }
            }

            if let Some(b) = right_bias_opt {
                last_right = Some((now, b));
                right_biases.push(b);
            } else if let Some((t, b)) = last_right {
                if now - t <= DECAY_SECONDS {
                    right_biases.push(b);
                }
            }

            current_time += sample_interval;
        }

        let avg = |vals: &[f64]| -> f64 {
            if vals.is_empty() {
                0.15
            } else {
                (vals.iter().sum::<f64>() / vals.len() as f64).clamp(0.0, 1.0)
            }
        };

        Ok((avg(&left_biases), avg(&right_biases)))
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
        _duration: f64,
    ) -> Option<(BoundingBox, BoundingBox)> {
        const MARGIN: f64 = 0.25;

        if frames.is_empty() {
            return None;
        }

        use std::collections::HashMap;
        // Aggregate boxes per track, then deterministically map by center_x:
        // leftmost -> top panel, rightmost -> bottom panel.
        let mut track_boxes: HashMap<u32, Vec<BoundingBox>> = HashMap::new();
        for frame in frames {
            for det in frame {
                track_boxes.entry(det.track_id).or_default().push(det.bbox);
            }
        }

        if track_boxes.len() < 2 {
            return None;
        }

        let mut tracks: Vec<(u32, BoundingBox)> = track_boxes
            .iter()
            .filter_map(|(id, boxes)| BoundingBox::union(boxes).map(|b| (*id, b)))
            .collect();

        if tracks.len() < 2 {
            return None;
        }

        tracks.sort_by(|a, b| {
            a.1
                .cx()
                .partial_cmp(&b.1.cx())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let left_union = tracks[0].1;
        let right_union = tracks[1].1;

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
    fn test_speaker_split_left_top_right_bottom_invariant() {
        let width = 1920;
        let height = 1080;
        let frames = vec![vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(100.0, 200.0, 150.0, 150.0),
                0.9,
                1,
                Some(0.01),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1400.0, 220.0, 150.0, 150.0),
                0.9,
                2,
                Some(0.2),
            ),
        ]];

        let res = TierAwareSplitProcessor::evaluate_speaker_split(&frames, width, height, 0.5);
        assert!(res.is_some(), "Should enter split mode with two faces");
        let (left_box, right_box) = res.unwrap();
        assert!(left_box.cx() < right_box.cx(), "Left face should map to top panel");
    }

    #[test]
    fn test_evaluate_speaker_split_two_speakers_even_when_quiet() {
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
        assert!(res.is_some(), "Should still map two speakers visually");
    }
}
