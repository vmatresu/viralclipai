//! Video stacking utilities for split-view rendering.
//!
//! Provides functions to vertically stack two video halves into a portrait output.

use std::path::Path;

use tokio::process::Command;
use tracing::debug;

use crate::error::{MediaError, MediaResult};
use vclip_models::EncodingConfig;

/// Default dimensions for stacked panels (9:16 portrait split in half).
pub const DEFAULT_PANEL_WIDTH: u32 = 1080;
pub const DEFAULT_PANEL_HEIGHT: u32 = 960;

/// Configuration for stacking operations.
#[derive(Debug, Clone)]
pub struct StackingConfig {
    /// Width of each panel after normalization.
    pub panel_width: u32,
    /// Height of each panel after normalization.
    pub panel_height: u32,
}

impl Default for StackingConfig {
    fn default() -> Self {
        Self {
            panel_width: DEFAULT_PANEL_WIDTH,
            panel_height: DEFAULT_PANEL_HEIGHT,
        }
    }
}

impl StackingConfig {
    /// Create a custom stacking configuration.
    pub fn new(panel_width: u32, panel_height: u32) -> Self {
        Self { panel_width, panel_height }
    }

    /// Build the FFmpeg filter for normalizing and stacking two inputs.
    ///
    /// Each input is:
    /// - Timestamps normalized with setpts=PTS-STARTPTS to avoid discontinuities
    /// - Scaled to fit within the panel dimensions (maintaining aspect ratio)
    /// - Padded to exactly the panel dimensions (centered)
    /// - Set to square pixels (SAR 1:1)
    /// - Converted to yuv420p pixel format
    fn build_vstack_filter(&self) -> String {
        // Key fix: setpts=PTS-STARTPTS resets timestamps to prevent PTS discontinuities
        // that cause the "garbled flash" artifact at layout transitions
        format!(
            "[0:v]setpts=PTS-STARTPTS,scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p[top];\
             [1:v]setpts=PTS-STARTPTS,scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p[bottom];\
             [top][bottom]vstack=inputs=2[vout]",
            w = self.panel_width,
            h = self.panel_height
        )
    }
}

/// Stack two pre-cropped halves into a single portrait stream.
///
/// Uses default dimensions (1080x960 per panel = 1080x1920 output).
///
/// We explicitly map only the stacked video output and a single audio stream
/// to avoid ffmpeg's default behavior of muxing extra input streams (which
/// doubles file size). Audio is taken from the first input if present.
///
/// # Input Normalization
///
/// Both halves are scaled/padded to exactly 1080x960 with square pixels (SAR 1:1)
/// to prevent dimension mismatch errors from vstack when inputs have slight
/// resolution differences.
pub async fn stack_halves(
    top_half: &Path,
    bottom_half: &Path,
    output: &Path,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    stack_halves_with_config(top_half, bottom_half, output, encoding, &StackingConfig::default()).await
}

/// Stack two pre-cropped halves with custom panel dimensions.
///
/// This is the configurable version that allows specifying custom panel sizes.
pub async fn stack_halves_with_config(
    top_half: &Path,
    bottom_half: &Path,
    output: &Path,
    encoding: &EncodingConfig,
    config: &StackingConfig,
) -> MediaResult<()> {
    let stack_crf = encoding.crf.saturating_add(2);
    let filter = config.build_vstack_filter();

    debug!(
        "Stacking {} + {} -> {} (panel: {}x{})",
        top_half.display(),
        bottom_half.display(),
        output.display(),
        config.panel_width,
        config.panel_height
    );

    let stack_args = build_ffmpeg_args(top_half, bottom_half, output, encoding, &filter, stack_crf);
    let stack_status = Command::new("ffmpeg").args(&stack_args).output().await?;

    if !stack_status.status.success() {
        return Err(MediaError::ffmpeg_failed(
            "Stacking failed",
            Some(String::from_utf8_lossy(&stack_status.stderr).to_string()),
            stack_status.status.code(),
        ));
    }

    Ok(())
}

/// Build FFmpeg arguments for stacking.
fn build_ffmpeg_args(
    top_half: &Path,
    bottom_half: &Path,
    output: &Path,
    encoding: &EncodingConfig,
    filter: &str,
    crf: u8,
) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(),
        top_half.to_string_lossy().to_string(),
        "-i".to_string(),
        bottom_half.to_string_lossy().to_string(),
        "-filter_complex".to_string(),
        filter.to_string(),
        // Explicit stream mapping: keep only the stacked video and first input audio (if any)
        "-map".to_string(),
        "[vout]".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-c:v".to_string(),
        encoding.codec.clone(),
        "-preset".to_string(),
        encoding.preset.clone(),
        "-crf".to_string(),
        crf.to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        encoding.audio_bitrate.clone(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        "-shortest".to_string(),
        "-map_metadata".to_string(),
        "-1".to_string(),
        output.to_string_lossy().to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_stacking_config() {
        let config = StackingConfig::default();
        assert_eq!(config.panel_width, 1080);
        assert_eq!(config.panel_height, 960);
    }

    #[test]
    fn test_custom_stacking_config() {
        let config = StackingConfig::new(720, 640);
        assert_eq!(config.panel_width, 720);
        assert_eq!(config.panel_height, 640);
    }

    #[test]
    fn test_vstack_filter_generation() {
        let config = StackingConfig::default();
        let filter = config.build_vstack_filter();

        assert!(filter.contains("scale=1080:960"));
        assert!(filter.contains("pad=1080:960"));
        assert!(filter.contains("setsar=1"));
        assert!(filter.contains("format=yuv420p"));
        assert!(filter.contains("vstack=inputs=2"));
    }

    #[test]
    fn test_vstack_filter_custom_dimensions() {
        let config = StackingConfig::new(720, 480);
        let filter = config.build_vstack_filter();

        assert!(filter.contains("scale=720:480"));
        assert!(filter.contains("pad=720:480"));
    }
}
