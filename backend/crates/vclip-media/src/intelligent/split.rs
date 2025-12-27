//! Intelligent Split video processing.
//!
//! **DEPRECATED**: This module is superseded by `tier_aware_split.rs`.
//! The `TierAwareSplitProcessor` in `tier_aware_split.rs` provides the same
//! functionality with better tier support and cache integration.
//!
//! This module is kept for backward compatibility but should not be used
//! for new code. Use `create_tier_aware_split_clip_with_cache` instead.
//!
//! This module implements the "intelligent split" style that:
//! 1. Splits the video into left and right halves
//! 2. Applies face-centered crop to each half (9:16 portrait)
//! 3. Stacks the cropped halves vertically (left=top, right=bottom)
//!
//! This is ideal for podcast-style videos with two people side by side.
//!
//! # Architecture
//!
//! ```text
//! Video Input
//!     │
//!     ▼
//! ┌─────────────────┐
//! │  Split L/R      │ ← Crop left half, crop right half
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ Face-Center Crop│ ← Crop each half to 9:16 centered on face
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  VStack         │ ← Left on top, Right on bottom
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Scale 1080x1920│ ← Scale to standard portrait
//! └─────────────────┘
//! ```

use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

use super::config::IntelligentCropConfig;
use super::detection_adapter::{compute_vertical_bias, get_detections};
use super::models::{BoundingBox, Detection};
use super::TierAwareIntelligentCropper;
use crate::clip::extract_segment;
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::MediaResult;
use crate::intelligent::output_format::{
    clamp_crop_to_frame, SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH,
};
use crate::intelligent::stacking::stack_halves;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig, SceneNeuralAnalysis};

/// Layout mode for the output (kept for API compatibility).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitLayout {
    /// Split view with two panels - top and bottom
    SplitTopBottom,
    /// Single full frame with face tracking (not used in current implementation)
    #[allow(dead_code)]
    FullFrame,
}

#[derive(Debug, Default)]
struct SplitAnalysis {
    /// Number of distinct face tracks detected across the clip
    distinct_tracks: usize,
    /// Time (seconds) with 2+ faces visible SIMULTANEOUSLY
    /// This is critical: we only want split mode for true side-by-side podcasts,
    /// NOT for videos that switch between showing one person then another.
    simultaneous_face_time: f64,
    left_vertical_bias: f64,
    right_vertical_bias: f64,
    /// Average horizontal center of left face as fraction of left half width (0.0-1.0)
    left_horizontal_center: f64,
    /// Average horizontal center of right face as fraction of right half width (0.0-1.0)
    right_horizontal_center: f64,
}

/// Intelligent Split processor.
pub struct IntelligentSplitProcessor {
    #[allow(dead_code)]
    config: IntelligentCropConfig,
    tier: DetectionTier,
}

impl IntelligentSplitProcessor {
    /// Create a new processor with the given configuration.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self {
            config,
            tier: DetectionTier::Basic,
        }
    }

    /// Create with default configuration.
    pub fn default() -> Self {
        Self::new(IntelligentCropConfig::default())
    }

    /// Create with default config and explicit detection tier.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self {
            config: IntelligentCropConfig::default(),
            tier,
        }
    }

    /// Process a video segment with intelligent split.
    ///
    /// # Arguments
    /// * `segment_path` - Path to pre-cut video segment
    /// * `output_path` - Path for output file
    /// * `encoding` - Encoding configuration
    ///
    /// # Returns
    /// The determined layout mode used
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment_path: P,
        output_path: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<SplitLayout> {
        self.process_with_cached_detections(segment_path, output_path, encoding, None)
            .await
    }

    /// Process a video segment with optional cached neural analysis.
    ///
    /// This is the cache-aware entry point that allows skipping expensive ML inference
    /// when cached detections are available.
    pub async fn process_with_cached_detections<P: AsRef<Path>>(
        &self,
        segment_path: P,
        output_path: P,
        encoding: &EncodingConfig,
        cached_analysis: Option<&SceneNeuralAnalysis>,
    ) -> MediaResult<SplitLayout> {
        let segment = segment_path.as_ref();
        let output = output_path.as_ref();

        info!(
            "Analyzing video for intelligent split: {:?} (cached: {})",
            segment,
            cached_analysis.is_some()
        );

        // 1. Get video metadata
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, fps, duration
        );

        // Get detections from cache or run fallback detection
        let detections = get_detections(
            cached_analysis,
            segment,
            self.tier,
            0.0,
            duration,
            width,
            height,
            fps,
        )
        .await?;
        let analysis = self.analyze_detections(&detections, width, height);

        // KEY DECISION: Only use split mode for TRUE side-by-side podcasts.
        // Requirements:
        // 1. At least 2 distinct face tracks detected
        // 2. At least 3 seconds with BOTH faces visible SIMULTANEOUSLY
        let min_simultaneous_time = 3.0; // seconds
        let should_split = analysis.distinct_tracks >= 2
            && analysis.simultaneous_face_time >= min_simultaneous_time;

        if !should_split {
            info!(
                "Using full-frame mode: {} tracks, {:.2}s simultaneous (need >= {:.1}s for split) → tier {:?}",
                analysis.distinct_tracks,
                analysis.simultaneous_face_time,
                min_simultaneous_time,
                self.tier
            );
            // Use full-frame intelligent crop which dynamically follows faces
            let cropper = TierAwareIntelligentCropper::new(self.config.clone(), self.tier);
            cropper
                .process_with_cached_detections(segment, output, encoding, None, cached_analysis)
                .await?;
            self.generate_thumbnail(output).await;
            info!("Intelligent split (full-frame path) complete: {:?}", output);
            return Ok(SplitLayout::FullFrame);
        }

        info!(
            "Using split mode: {} tracks, {:.2}s simultaneous → splitting left/right",
            analysis.distinct_tracks, analysis.simultaneous_face_time
        );

        info!("Step 1/4: Splitting video into left/right halves...");
        info!("Step 2/4: Applying face-centered crop to each panel...");
        info!("Step 3/4: Stacking panels...");
        info!("Step 4/4: Scaling to portrait format...");

        self.process_split_view(segment, output, width, height, &analysis, encoding)
            .await?;

        // 3. Generate thumbnail
        self.generate_thumbnail(output).await;

        info!("Intelligent split complete: {:?}", output);
        Ok(SplitLayout::SplitTopBottom)
    }

    // NOTE: analyze_layout was removed - IntelligentSplit now always splits into
    // left/right halves without trying to detect face positions first.
    // This matches the Python implementation behavior which works correctly.

    /// Process as split view with two panels (left half → top, right half → bottom).
    ///
    /// This function:
    /// 1. Computes crop regions dynamically centered on detected face positions
    /// 2. Applies face-centered crop to each portion with 9:16 portrait aspect
    /// 3. Stacks the cropped portions vertically (left=top, right=bottom)
    /// 4. Scales to final 1080x1920 output
    ///
    /// The key improvement: Crops are centered on actual detected face positions,
    /// not just a fixed percentage from each edge.
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        analysis: &SplitAnalysis,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        // Create temp directory for intermediate files
        let temp_dir = tempfile::tempdir()?;

        let half_width = width as f64 / 2.0;

        // Target crop width: we want each panel to be 9:8 aspect ratio (for stacking to 9:16)
        // The crop width should be tall enough to capture faces well
        // Target: capture ~50-55% of the frame width for each person, centered on their face
        let target_crop_fraction = 0.50; // Capture 50% width per person
        let crop_width_f = (width as f64 * target_crop_fraction).min(half_width * 1.1);
        let crop_width = crop_width_f.round() as u32;

        // Calculate 9:8 tile dimensions
        let tile_height = ((crop_width as f64 * 8.0 / 9.0).round() as u32).min(height);
        let vertical_margin = height.saturating_sub(tile_height);

        // === LEFT HALF CROP ===
        // Center the crop on the detected face position
        // left_horizontal_center is 0.0-1.0 within the left half
        let left_face_x = half_width * analysis.left_horizontal_center;

        // Compute crop start X: center the crop on the face, but clamp to not go outside left half
        let left_crop_x = (left_face_x - crop_width_f / 2.0)
            .max(0.0)
            .min(half_width - crop_width_f.min(half_width))
            .round() as u32;

        // Vertical bias
        let top_crop_y = (vertical_margin as f64 * analysis.left_vertical_bias).round() as u32;

        // Clamp crop coordinates to ensure validity
        let (left_crop_x, top_crop_y, crop_width, tile_height) = clamp_crop_to_frame(
            left_crop_x as i32,
            top_crop_y as i32,
            crop_width as i32,
            tile_height as i32,
            width,
            height,
        );

        // === RIGHT HALF CROP ===
        // Center the crop on the detected face position
        // right_horizontal_center is 0.0-1.0 within the right half
        let right_face_x = half_width + half_width * analysis.right_horizontal_center;

        // Compute crop start X: center the crop on the face, but clamp to stay within right half
        let right_crop_x = (right_face_x - crop_width_f / 2.0)
            .max(half_width)
            .min(width as f64 - crop_width_f)
            .round() as u32;

        // Vertical bias
        let bottom_crop_y = (vertical_margin as f64 * analysis.right_vertical_bias).round() as u32;

        // Clamp crop coordinates to ensure validity
        let (right_crop_x, bottom_crop_y, crop_width, tile_height) = clamp_crop_to_frame(
            right_crop_x as i32,
            bottom_crop_y as i32,
            crop_width as i32,
            tile_height as i32,
            width,
            height,
        );

        // Step 1: Extract left and right portions with face-centered crops
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        info!(
            "  Extracting left person (x={} to {}, face at {:.0}%)",
            left_crop_x,
            left_crop_x + crop_width,
            analysis.left_horizontal_center * 100.0
        );

        // Left person: single crop centered on face with proper aspect ratio
        let left_filter = format!(
            "crop={}:{}:{}:{},scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
            crop_width, tile_height, left_crop_x, top_crop_y,  // Single crop to panel aspect ratio
            SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT,  // Scale to panel dimensions
            SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT  // Pad to panel dimensions
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
            "  Extracting right person (x={} to {}, face at {:.0}%)",
            right_crop_x,
            right_crop_x + crop_width,
            analysis.right_horizontal_center * 100.0
        );

        // Right person: single crop centered on face with proper aspect ratio
        let right_filter = format!(
            "crop={}:{}:{}:{},scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
            crop_width, tile_height, right_crop_x, bottom_crop_y,  // Single crop to panel aspect ratio
            SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT,  // Scale to panel dimensions
            SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT  // Pad to panel dimensions
        );

        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter(&right_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_right).await?;

        // Step 2: Stack the halves vertically (left=top, right=bottom)
        // Both are now 1080x960 (9:8), stacking gives final 1080x1920
        info!("  Stacking panels...");
        stack_halves(&left_half, &right_half, output, encoding).await
    }

    /// Analyze detections to understand face presence and positioning.
    fn analyze_detections(
        &self,
        detections: &[Vec<Detection>],
        width: u32,
        height: u32,
    ) -> SplitAnalysis {
        if detections.is_empty() {
            return SplitAnalysis::default();
        }

        let sample_interval = if self.config.fps_sample > 0.0 {
            1.0 / self.config.fps_sample
        } else {
            0.125
        };

        let center_x = width as f64 / 2.0;
        let half_width = center_x;
        let mut track_presence: HashMap<u32, f64> = HashMap::new();
        let mut simultaneous_face_time = 0.0;
        let mut left_faces: Vec<BoundingBox> = Vec::new();
        let mut right_faces: Vec<BoundingBox> = Vec::new();

        for frame_dets in detections {
            // Track time when 2+ faces are visible SIMULTANEOUSLY
            // This is the key metric for determining if split mode is appropriate
            if frame_dets.len() >= 2 {
                simultaneous_face_time += sample_interval;
            }
            for det in frame_dets {
                *track_presence.entry(det.track_id).or_insert(0.0) += sample_interval;
                if det.bbox.cx() < center_x {
                    left_faces.push(det.bbox);
                } else {
                    right_faces.push(det.bbox);
                }
            }
        }

        let (left_vertical_bias, right_vertical_bias) =
            self.compute_vertical_biases(&left_faces, &right_faces, height);

        // Compute horizontal centers relative to each half
        let left_horizontal_center = if left_faces.is_empty() {
            0.5 // Default to center of left half
        } else {
            let avg_cx: f64 =
                left_faces.iter().map(|f| f.cx()).sum::<f64>() / left_faces.len() as f64;
            // Normalize to 0.0-1.0 within the left half
            (avg_cx / half_width).max(0.1).min(0.9)
        };

        let right_horizontal_center = if right_faces.is_empty() {
            0.5 // Default to center of right half
        } else {
            let avg_cx: f64 =
                right_faces.iter().map(|f| f.cx()).sum::<f64>() / right_faces.len() as f64;
            // Normalize to 0.0-1.0 within the right half (cx is from center_x to width)
            ((avg_cx - center_x) / half_width).max(0.1).min(0.9)
        };

        SplitAnalysis {
            distinct_tracks: track_presence.len(),
            simultaneous_face_time,
            left_vertical_bias,
            right_vertical_bias,
            left_horizontal_center,
            right_horizontal_center,
        }
    }

    fn compute_vertical_biases(
        &self,
        left_faces: &[BoundingBox],
        right_faces: &[BoundingBox],
        height: u32,
    ) -> (f64, f64) {
        (
            compute_vertical_bias(left_faces, height),
            compute_vertical_bias(right_faces, height),
        )
    }

    async fn generate_thumbnail(&self, output: &Path) {
        info!("Step 3/3: Generating thumbnail...");
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            warn!("Failed to generate thumbnail: {}", e);
        }
    }
}

/// Create an intelligent split clip from a video file.
///
/// This is the main entry point for the IntelligentSplit style.
///
/// # Behavior
/// Always creates a split view by:
/// 1. Splitting the video into left and right halves
/// 2. Stacking the cropped halves vertically (left=top, right=bottom)
/// 3. Applying intelligent face-tracking crop to the stacked result
///
/// This is ideal for podcast-style videos with two people side by side.
pub async fn create_intelligent_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    create_intelligent_split_clip_with_cache(input, output, task, encoding, None, progress_callback)
        .await
}

/// Create an intelligent split clip with optional cached neural analysis.
///
/// This is the cache-aware entry point that allows skipping expensive ML inference
/// when cached detections are available.
pub async fn create_intelligent_split_clip_with_cache<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    cached_analysis: Option<&SceneNeuralAnalysis>,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Creating intelligent split clip: {} -> {} (cached: {})",
        input.display(),
        output.display(),
        cached_analysis.is_some()
    );

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    // Step 1: Extract segment to temporary file
    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for intelligent split: {:.2}s - {:.2}s",
        start_secs, end_secs
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with intelligent split (using cache if available)
    let config = IntelligentCropConfig::default();
    let processor = IntelligentSplitProcessor::new(config);
    let result = processor
        .process_with_cached_detections(segment_path.as_path(), output, encoding, cached_analysis)
        .await;

    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            warn!(
                "Failed to delete temporary segment file {}: {}",
                segment_path.display(),
                e
            );
        } else {
            info!("Deleted temporary segment: {}", segment_path.display());
        }
    }

    result.map(|_| ())
}
