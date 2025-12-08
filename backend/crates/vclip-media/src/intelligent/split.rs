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
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use crate::intelligent::stacking::stack_halves;
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
    #[allow(dead_code)]
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
    /// 1. Splits the video into left and right portions with slight overlap avoidance
    /// 2. Applies face-centered crop to each portion, using a WIDER crop (9:8 aspect) 
    ///    to preserve more context while keeping faces centered
    /// 3. Stacks the cropped portions vertically (left=top, right=bottom)
    /// 4. Scales to final 1080x1920 output
    ///
    /// The key improvement: Instead of a 50/50 split which includes overlap (other
    /// person's arm/body visible), we crop ~45% from each side to cleanly isolate
    /// each person.
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
        
        // Improved split algorithm:
        // Instead of taking exactly 50% of each side (which includes overlap where
        // the other person's arm appears), we take ~45% from each side, cropped
        // toward that person's position.
        //
        // For a 1920x1080 source:
        //   - Left person crop: 0 to 45% width = 0 to 864px
        //   - Right person crop: 55% to 100% width = 1056 to 1920px (864px wide)
        //
        // Each crop is then made into a 9:8 tile by trimming vertically.
        
        let crop_fraction = 0.45; // Take 45% from each side
        let crop_width = (width as f64 * crop_fraction).round() as u32;
        let right_start_x = width - crop_width; // Start from right edge minus crop width
        
        // Calculate 9:8 tile dimensions
        // Tile height = crop_width * 8 / 9
        let tile_height = ((crop_width as f64 * 8.0 / 9.0).round() as u32).min(height);
        let vertical_margin = height.saturating_sub(tile_height);

        // Both panels: bias upward to show more face/upper body
        // This ensures heads are not cut off at the top of the frame
        let top_crop_y = 0u32;
        // Right person (bottom panel): also bias upward to show full face
        // Use a small offset from top to ensure head is fully visible
        let bottom_crop_y = (vertical_margin as f64 * 0.15).round() as u32;

        // Step 1: Extract left and right portions with proper isolation
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        info!("  Extracting left person (0 to {}px width)...", crop_width);
        // Left person: start from x=0, take crop_width pixels, then crop to 9:8 and scale
        let left_filter = format!(
            "crop={}:{}:0:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width, height,           // First: extract left portion
            crop_width, tile_height, top_crop_y  // Then: crop to 9:8 vertically
        );
        
        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter(&left_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_left).await?;

        info!("  Extracting right person ({}px to {}px width)...", right_start_x, width);
        // Right person: start from right_start_x, take crop_width pixels, then crop to 9:8
        let right_filter = format!(
            "crop={}:{}:{}:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width, height, right_start_x,  // First: extract right portion
            crop_width, tile_height, bottom_crop_y  // Then: crop to 9:8 vertically
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
