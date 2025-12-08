//! Fast split engine for heuristic-based video splitting.
//!
//! This module extracts the fast/deterministic split algorithm that doesn't
//! rely on AI face detection. It splits landscape videos into left/right
//! halves and stacks them vertically using fixed geometric positioning.
//!
//! # Algorithm
//!
//! 1. Take 45% from each side of the video (left person: 0-45%, right person: 55-100%)
//! 2. Crop each half to 9:8 aspect ratio
//! 3. Scale each half to 1080x960
//! 4. Stack vertically to produce 1080x1920 output
//!
//! This matches the Python implementation's approach for the simple split style.

use std::path::Path;
use tracing::info;

use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use crate::intelligent::stacking::stack_halves;
use vclip_models::EncodingConfig;

/// Configuration for the fast split engine.
#[derive(Debug, Clone)]
pub struct FastSplitConfig {
    /// Fraction of video width to crop from each side (default: 0.45 = 45%)
    pub crop_fraction: f64,
    /// Vertical position for top panel crop (0.0 = top, 1.0 = bottom)
    pub top_vertical_bias: f64,
    /// Vertical position for bottom panel crop (0.5 = center)
    pub bottom_vertical_bias: f64,
}

impl Default for FastSplitConfig {
    fn default() -> Self {
        Self {
            crop_fraction: 0.45,
            top_vertical_bias: 0.0,     // Bias upward for top panel (face/upper body)
            bottom_vertical_bias: 0.15, // Also bias upward for bottom panel to show full face
        }
    }
}

/// Fast split engine using heuristic positioning only.
///
/// This engine splits landscape videos into left/right halves and stacks
/// them vertically, using fixed geometric positioning. No face detection
/// is used - purely deterministic crop positions.
///
/// # Use Cases
/// - Fast processing when AI detection is not needed
/// - Fallback when YuNet models are unavailable
/// - Podcast-style videos with consistent speaker positions
pub struct FastSplitEngine {
    config: FastSplitConfig,
}

impl FastSplitEngine {
    /// Create a new fast split engine with default configuration.
    pub fn new() -> Self {
        Self {
            config: FastSplitConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: FastSplitConfig) -> Self {
        Self { config }
    }

    /// Process a video segment with fast split.
    ///
    /// # Arguments
    /// * `segment` - Path to pre-cut video segment
    /// * `output` - Path for output file
    /// * `encoding` - Encoding configuration
    ///
    /// # Returns
    /// Ok(()) on success
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();

        info!("Fast split processing: {:?}", segment);

        // 1. Get video metadata
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, video_info.fps, video_info.duration
        );

        // 2. Process with fixed split algorithm
        self.process_split_view(segment, output, width, height, encoding)
            .await?;

        // 3. Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("Failed to generate thumbnail: {}", e);
        }

        info!("Fast split complete: {:?}", output);
        Ok(())
    }

    /// Process the video with fixed left/right split and vertical stack.
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        // Create temp directory for intermediate files
        let temp_dir = tempfile::tempdir()?;

        // Calculate crop dimensions
        // Take crop_fraction from each side to avoid overlap in the middle
        let crop_width = (width as f64 * self.config.crop_fraction).round() as u32;
        let right_start_x = width - crop_width;

        // Calculate 9:8 tile dimensions for each half
        let tile_height = ((crop_width as f64 * 8.0 / 9.0).round() as u32).min(height);
        let vertical_margin = height.saturating_sub(tile_height);

        // Top panel (left person): bias upward based on config
        let top_crop_y = (vertical_margin as f64 * self.config.top_vertical_bias).round() as u32;
        // Bottom panel (right person): center or bias based on config
        let bottom_crop_y =
            (vertical_margin as f64 * self.config.bottom_vertical_bias).round() as u32;

        // Step 1: Extract left and right portions
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        info!(
            "  Extracting left person (0 to {}px width)...",
            crop_width
        );

        // Left person: start from x=0, crop to 9:8 and scale to 1080x960
        let left_filter = format!(
            "crop={}:{}:0:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width,
            height, // First: extract left portion
            crop_width,
            tile_height,
            top_crop_y // Then: crop to 9:8 vertically
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
            "  Extracting right person ({}px to {}px width)...",
            right_start_x, width
        );

        // Right person: start from right_start_x, crop to 9:8 and scale
        let right_filter = format!(
            "crop={}:{}:{}:0,crop={}:{}:0:{},scale=1080:960:flags=lanczos",
            crop_width,
            height,
            right_start_x, // First: extract right portion
            crop_width,
            tile_height,
            bottom_crop_y // Then: crop to 9:8 vertically
        );

        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter(&right_filter)
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&encoding.audio_bitrate);

        FfmpegRunner::new().run(&cmd_right).await?;

        info!("  Stacking panels...");
        stack_halves(&left_half, &right_half, output, encoding).await
    }
}

impl Default for FastSplitEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FastSplitConfig::default();
        assert!((config.crop_fraction - 0.45).abs() < 0.001);
        assert!((config.top_vertical_bias - 0.0).abs() < 0.001);
        assert!((config.bottom_vertical_bias - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_crop_dimensions() {
        // For a 1920x1080 video with 45% crop:
        let width = 1920u32;
        let height = 1080u32;
        let crop_fraction = 0.45;

        let crop_width = (width as f64 * crop_fraction).round() as u32;
        assert_eq!(crop_width, 864);

        let right_start_x = width - crop_width;
        assert_eq!(right_start_x, 1056);

        // Tile height for 9:8 aspect
        let tile_height = ((crop_width as f64 * 8.0 / 9.0).round() as u32).min(height);
        assert_eq!(tile_height, 768);
    }
}
