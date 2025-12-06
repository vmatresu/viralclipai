//! Intelligent Split video processing.
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

use std::path::Path;
use tracing::{info, warn};

use super::config::IntelligentCropConfig;
use super::IntelligentCropper;
use crate::clip::extract_segment;
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use vclip_models::{ClipTask, EncodingConfig};

/// Layout mode for the output (kept for API compatibility).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitLayout {
    /// Split view with two panels - top and bottom
    SplitTopBottom,
    /// Single full frame with face tracking (not used in current implementation)
    #[allow(dead_code)]
    FullFrame,
}

/// Intelligent Split processor.
pub struct IntelligentSplitProcessor {
    config: IntelligentCropConfig,
}

impl IntelligentSplitProcessor {
    /// Create a new processor with the given configuration.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration.
    pub fn default() -> Self {
        Self::new(IntelligentCropConfig::default())
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
        let segment = segment_path.as_ref();
        let output = output_path.as_ref();

        info!("Analyzing video for intelligent split: {:?}", segment);

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

        // IntelligentSplit ALWAYS splits into left/right halves and applies
        // intelligent crop to each half. This matches the Python implementation
        // which works well for podcast-style videos with two people.
        //
        // The layout analysis was removed because:
        // 1. Face detection on full frame often classifies both people as "center"
        // 2. The Python version always splits and it works correctly
        // 3. Users expect split view when they select "Intelligent Split"
        
        info!("Step 1/4: Splitting video into left/right halves...");
        info!("Step 2/4: Applying face-centered crop to each panel...");
        info!("Step 3/4: Stacking panels...");
        info!("Step 4/4: Scaling to portrait format...");
        
        self.process_split_view(
            segment,
            output,
            width,
            height,
            fps,
            duration,
            encoding,
        )
        .await?;

        // 3. Generate thumbnail
        info!("Step 3/3: Generating thumbnail...");
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            warn!("Failed to generate thumbnail: {}", e);
        }

        info!("Intelligent split complete: {:?}", output);
        Ok(SplitLayout::SplitTopBottom) // Always returns split layout now
    }

    // NOTE: analyze_layout was removed - IntelligentSplit now always splits into
    // left/right halves without trying to detect face positions first.
    // This matches the Python implementation behavior which works correctly.

    /// Process as split view with two panels (left half → top, right half → bottom).
    ///
    /// This function:
    /// 1. Splits the video into left and right halves
    /// 2. Applies intelligent face-centered crop to each half (9:16 portrait)
    /// 3. Stacks the cropped halves vertically (left=top, right=bottom)
    /// 4. Scales to final 1080x1920 output
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        _width: u32,
        _height: u32,
        _fps: f64,
        _duration: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        // Create temp directory for intermediate files
        let temp_dir = tempfile::tempdir()?;

        // Step 1: Extract left and right halves
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        info!("  Extracting left half...");
        // Crop left half
        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter("crop=iw/2:ih:0:0")
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("copy");

        FfmpegRunner::new().run(&cmd_left).await?;

        info!("  Extracting right half...");
        // Crop right half
        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter("crop=iw/2:ih:iw/2:0")
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("copy");

        FfmpegRunner::new().run(&cmd_right).await?;

        // Step 2: Apply intelligent face-centered crop to each half
        // Each half is now (width/2) x height, we need to crop to 9:16 portrait
        // centered on the detected face
        let left_cropped = temp_dir.path().join("left_cropped.mp4");
        let right_cropped = temp_dir.path().join("right_cropped.mp4");

        info!("  Applying face-centered crop to left panel...");
        let cropper = IntelligentCropper::new(self.config.clone());
        cropper.process(&left_half, &left_cropped).await?;

        info!("  Applying face-centered crop to right panel...");
        cropper.process(&right_half, &right_cropped).await?;

        // Step 3: Stack the face-cropped halves vertically (left=top, right=bottom)
        // Both halves are now 9:16 portrait, so stacking gives us a 9:32 video
        let stacked = temp_dir.path().join("stacked.mp4");
        info!("  Stacking panels...");
        let stack_crf = encoding.crf.saturating_add(4);

        let stack_args = vec![
            "-y".to_string(),
            "-i".to_string(),
            left_cropped.to_string_lossy().to_string(),
            "-i".to_string(),
            right_cropped.to_string_lossy().to_string(),
            "-filter_complex".to_string(),
            // Scale both to same width before stacking to ensure alignment
            "[0:v]scale=1080:-2[v0];[1:v]scale=1080:-2[v1];[v0][v1]vstack=inputs=2".to_string(),
            "-c:v".to_string(),
            encoding.codec.clone(),
            "-preset".to_string(),
            encoding.preset.clone(),
            "-crf".to_string(),
            stack_crf.to_string(),
            "-c:a".to_string(),
            encoding.audio_codec.clone(),
            "-b:a".to_string(),
            encoding.audio_bitrate.clone(),
            stacked.to_string_lossy().to_string(),
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

        // Step 4: Scale to standard 9:16 portrait dimensions (1080x1920)
        // The stacked video is 1080x(2*panel_height), we need to fit it into 1080x1920
        info!("  Scaling to 1080x1920 portrait format...");
        
        let scale_args = vec![
            "-y".to_string(),
            "-i".to_string(),
            stacked.to_string_lossy().to_string(),
            "-vf".to_string(),
            "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2".to_string(),
            "-c:v".to_string(),
            encoding.codec.clone(),
            "-preset".to_string(),
            encoding.preset.clone(),
            "-crf".to_string(),
            encoding.crf.to_string(),
            "-c:a".to_string(),
            encoding.audio_codec.clone(),
            "-b:a".to_string(),
            encoding.audio_bitrate.clone(),
            "-movflags".to_string(),
            "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        let scale_status = tokio::process::Command::new("ffmpeg")
            .args(&scale_args)
            .output()
            .await?;

        if !scale_status.status.success() {
            return Err(MediaError::ffmpeg_failed(
                "Scaling to portrait failed",
                Some(String::from_utf8_lossy(&scale_status.stderr).to_string()),
                scale_status.status.code(),
            ));
        }

        Ok(())
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
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_intelligent_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Creating intelligent split clip: {} -> {}",
        input.display(),
        output.display()
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

    // Step 2: Process with intelligent split
    let config = IntelligentCropConfig::default();
    let processor = IntelligentSplitProcessor::new(config);
    let result = processor.process(segment_path.as_path(), output, encoding).await;

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
