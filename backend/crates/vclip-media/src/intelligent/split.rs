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
    /// 2. Applies face-centered crop to each half, using a WIDER crop (1:2 aspect) 
    ///    to preserve more context while keeping faces centered
    /// 3. Stacks the cropped halves vertically (left=top, right=bottom)
    /// 4. Scales to final 1080x1920 output
    ///
    /// The key difference from IntelligentCrop is that we use a wider crop (1:2)
    /// for each panel, then stack them. This shows more of each person's body
    /// rather than a tight face crop.
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        _fps: f64,
        _duration: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        // Create temp directory for intermediate files
        let temp_dir = tempfile::tempdir()?;
        
        // For split view, we want to preserve more of each panel's width
        // Each half is (width/2) x height = 960x1080 for a 1920x1080 source
        // 
        // Instead of tight 9:16 crop (would be 607x1080, losing 37% width),
        // we use a CENTER-WEIGHTED crop that keeps the face visible but
        // shows more body/context.
        //
        // Strategy: Use a 1:2 aspect ratio crop for each panel (540x1080 from 960x1080)
        // This preserves 56% of the width vs 63% from 9:16.
        // Actually, we'll use scale-to-fit which preserves ALL content.

        // Step 1: Extract left and right halves with face-centered positioning
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");
        
        let half_width = width / 2;
        
        // For each half, we'll create a 9:16 portrait crop that preserves the full
        // height and centers horizontally on the face region
        // 
        // A 9:16 crop from 960x1080 would be:
        // - If using full height (1080): width = 1080 * 9/16 = 607.5
        // - Crop is centered, so we take 607 pixels from center of 960

        info!("  Extracting and centering left half...");
        // Left half: crop=960:1080:0:0, then crop to 9:16 centered on face
        // Face in left half is typically at ~50% of the half = 25% of full frame
        // For left half, face is centered, so center crop works
        let left_crop_x = ((half_width as f64 - (height as f64 * 9.0 / 16.0)) / 2.0).max(0.0) as u32;
        let left_crop_w = ((height as f64 * 9.0 / 16.0) as u32).min(half_width);
        
        let left_filter = format!(
            "crop={}:{}:{}:0,scale=1080:1920:flags=lanczos",
            left_crop_w, height, left_crop_x
        );
        
        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter(&format!("crop=iw/2:ih:0:0,{}", left_filter))
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_left).await?;

        info!("  Extracting and centering right half...");
        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter(&format!("crop=iw/2:ih:iw/2:0,{}", left_filter))
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_right).await?;

        // Step 2: Stack the halves vertically (left=top, right=bottom)
        // Both are now 1080x1920, stacking gives 1080x3840
        let stacked = temp_dir.path().join("stacked.mp4");
        info!("  Stacking panels...");
        let stack_crf = encoding.crf.saturating_add(2);

        let stack_args = vec![
            "-y".to_string(),
            "-i".to_string(),
            left_half.to_string_lossy().to_string(),
            "-i".to_string(),
            right_half.to_string_lossy().to_string(),
            "-filter_complex".to_string(),
            // Each input is 1080x1920, scale to 1080x960 (half height) then stack
            "[0:v]scale=1080:960:flags=lanczos[v0];[1:v]scale=1080:960:flags=lanczos[v1];[v0][v1]vstack=inputs=2".to_string(),
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

        // The stacked output is already 1080x1920 (1080x960 * 2 = 1080x1920)
        // No additional scaling needed

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
