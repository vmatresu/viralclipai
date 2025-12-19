//! Watermark overlay for free-tier video exports.
//!
//! This module provides functionality to apply a branded watermark
//! to videos for users on the free plan. The watermark includes
//! the Viral Clip AI logo and text in the bottom-right corner.
//!
//! # Architecture
//!
//! - `WatermarkConfig`: Builder pattern for overlay configuration
//! - `WatermarkService`: High-level API for watermark application
//! - `apply_watermark`: Low-level FFmpeg overlay function

use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::{MediaError, MediaResult};
use vclip_models::EncodingConfig;

// =============================================================================
// Constants
// =============================================================================

/// Default watermark asset path in production container.
pub const DEFAULT_WATERMARK_PATH: &str = "/app/assets/watermark.png";

/// Development fallback paths to check.
const DEV_WATERMARK_PATHS: &[&str] = &[
    "./backend/assets/watermark.png",
    "../backend/assets/watermark.png",
    "assets/watermark.png",
];

// =============================================================================
// Configuration (Builder Pattern)
// =============================================================================

/// Configuration for watermark overlay.
///
/// Use the builder pattern for flexible configuration:
/// ```ignore
/// let config = WatermarkConfig::default()
///     .with_offset(30, 30)
///     .with_opacity(0.8);
/// ```
#[derive(Debug, Clone)]
pub struct WatermarkConfig {
    /// Path to watermark image (PNG with transparency)
    pub image_path: String,
    /// Horizontal offset from right edge (pixels)
    pub offset_x: u32,
    /// Vertical offset from bottom edge (pixels)  
    pub offset_y: u32,
    /// Opacity (0.0 to 1.0)
    pub opacity: f32,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            image_path: resolve_watermark_path(),
            offset_x: 20,
            offset_y: 20,
            opacity: 0.7,
        }
    }
}

impl WatermarkConfig {
    /// Create config with custom image path.
    pub fn with_image_path(mut self, path: impl Into<String>) -> Self {
        self.image_path = path.into();
        self
    }

    /// Set offset from bottom-right corner.
    pub fn with_offset(mut self, x: u32, y: u32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    /// Set watermark opacity (0.0 = invisible, 1.0 = fully opaque).
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Check if the watermark image exists.
    pub fn is_available(&self) -> bool {
        Path::new(&self.image_path).exists()
    }

    /// Validate configuration.
    pub fn validate(&self) -> MediaResult<()> {
        if !self.is_available() {
            return Err(MediaError::InvalidVideo(format!(
                "Watermark image not found: {}",
                self.image_path
            )));
        }
        Ok(())
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Resolve watermark path, checking dev fallbacks if production path missing.
fn resolve_watermark_path() -> String {
    // Check production path first
    if Path::new(DEFAULT_WATERMARK_PATH).exists() {
        return DEFAULT_WATERMARK_PATH.to_string();
    }

    // Check development fallback paths
    for path in DEV_WATERMARK_PATHS {
        if Path::new(path).exists() {
            debug!(path = path, "Found watermark at dev fallback path");
            return path.to_string();
        }
    }

    // Return production path (will fail gracefully later)
    DEFAULT_WATERMARK_PATH.to_string()
}

/// Build FFmpeg filter complex for overlay.
fn build_overlay_filter(config: &WatermarkConfig) -> String {
    // Position formula: W-w-X puts overlay at (X pixels from right edge)
    // H-h-Y puts overlay at (Y pixels from bottom edge)
    if config.opacity < 1.0 {
        // Apply opacity via colorchannelmixer alpha channel
        format!(
            "[1:v]format=rgba,colorchannelmixer=aa={:.2}[wm];[0:v][wm]overlay=W-w-{}:H-h-{}:format=auto",
            config.opacity,
            config.offset_x,
            config.offset_y
        )
    } else {
        // Full opacity - simpler filter
        format!(
            "[0:v][1:v]overlay=W-w-{}:H-h-{}:format=auto",
            config.offset_x,
            config.offset_y
        )
    }
}

fn escape_filter_path(path: &str) -> String {
    path.replace('\\', "\\\\").replace('\'', "\\'").replace(':', "\\:")
}

fn build_movie_overlay_filter(
    config: &WatermarkConfig,
    input_label: &str,
    output_label: &str,
) -> String {
    let escaped_path = escape_filter_path(&config.image_path);
    if config.opacity < 1.0 {
        format!(
            "movie='{}',format=rgba,colorchannelmixer=aa={:.2}[wm];[{}][wm]overlay=W-w-{}:H-h-{}:format=auto[{}]",
            escaped_path,
            config.opacity,
            input_label,
            config.offset_x,
            config.offset_y,
            output_label
        )
    } else {
        format!(
            "movie='{}'[wm];[{}][wm]overlay=W-w-{}:H-h-{}:format=auto[{}]",
            escaped_path,
            input_label,
            config.offset_x,
            config.offset_y,
            output_label
        )
    }
}

/// Build a single-input filter graph with optional watermark overlay.
///
/// Uses the `movie` source filter so we can keep a single ffmpeg input.
pub fn build_vf_with_watermark(
    base_filter: Option<&str>,
    config: &WatermarkConfig,
) -> Option<String> {
    if !config.is_available() {
        return None;
    }

    let mut chains: Vec<String> = Vec::new();
    let base_label = if let Some(filter) = base_filter {
        chains.push(format!("[in]{}[base]", filter));
        "base"
    } else {
        "in"
    };

    chains.push(build_movie_overlay_filter(config, base_label, "out"));

    let filter = chains.join(";");
    Some(filter.replace("[out]", ""))
}

/// Append a watermark overlay to an existing filter complex.
pub struct WatermarkFilterComplex {
    pub filter_complex: String,
    pub output_label: String,
}

pub fn append_watermark_filter_complex(
    filter_complex: &str,
    input_label: &str,
    config: &WatermarkConfig,
) -> Option<WatermarkFilterComplex> {
    if !config.is_available() {
        return None;
    }

    let output_label = format!("{}_wm", input_label);
    let overlay = build_movie_overlay_filter(config, input_label, &output_label);

    Some(WatermarkFilterComplex {
        filter_complex: format!("{filter_complex};{overlay}"),
        output_label,
    })
}

// =============================================================================
// Core Functions
// =============================================================================

/// Apply watermark overlay to a video file (in-place).
///
/// Creates a temporary file with watermark, then atomically replaces the original.
///
/// # Arguments
/// * `video_path` - Path to video file (will be modified in-place)
/// * `config` - Watermark configuration
/// * `encoding` - Encoding settings for re-encoding
///
/// # Errors
/// Returns error if:
/// - Watermark image doesn't exist
/// - FFmpeg command fails
/// - File replacement fails
pub async fn apply_watermark(
    video_path: &Path,
    config: &WatermarkConfig,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    // Validate config
    config.validate()?;

    let video_str = video_path.to_string_lossy();
    let temp_output = video_path.with_extension("watermarked.mp4");
    let temp_output_str = temp_output.to_string_lossy();

    info!(
        video = %video_str,
        watermark = %config.image_path,
        opacity = config.opacity,
        "Applying watermark overlay"
    );

    // Build FFmpeg command
    let filter_complex = build_overlay_filter(config);
    
    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel", "warning",
            "-i", &video_str,
            "-i", &config.image_path,
            "-filter_complex", &filter_complex,
            "-c:v", &encoding.codec,
            "-preset", &encoding.preset,
            "-crf", &encoding.crf.to_string(),
            "-c:a", "copy",
            "-movflags", "+faststart",
            &temp_output_str,
        ])
        .output()
        .await
        .map_err(|e| MediaError::ffmpeg_failed(
            format!("Failed to spawn FFmpeg: {}", e),
            None,
            None,
        ))?;

    if !output.status.success() {
        // Clean up temp file on failure
        let _ = tokio::fs::remove_file(&temp_output).await;
        
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MediaError::ffmpeg_failed(
            "Watermark overlay failed",
            Some(stderr.into_owned()),
            output.status.code(),
        ));
    }

    // Atomic replace: rename temp file to original
    tokio::fs::rename(&temp_output, video_path).await.map_err(|e| {
        MediaError::InvalidVideo(format!(
            "Failed to replace video with watermarked version: {}",
            e
        ))
    })?;

    info!(video = %video_str, "Watermark applied successfully");
    Ok(())
}

/// Apply watermark if available, skipping gracefully if not.
///
/// This is a non-fatal wrapper for development environments where
/// the watermark asset may not be present.
///
/// # Returns
/// - `Ok(true)` if watermark was applied
/// - `Ok(false)` if watermark was skipped (asset missing)
/// - `Err` only for fatal errors during application
pub async fn apply_watermark_if_available(
    video_path: &Path,
    config: &WatermarkConfig,
    encoding: &EncodingConfig,
) -> MediaResult<bool> {
    // Skip if video doesn't exist
    if !video_path.exists() {
        warn!(
            video = %video_path.display(),
            "Skipping watermark: video file not found"
        );
        return Ok(false);
    }

    // Skip if watermark image doesn't exist
    if !config.is_available() {
        debug!(
            watermark = %config.image_path,
            "Skipping watermark: asset not found (expected in dev)"
        );
        return Ok(false);
    }

    apply_watermark(video_path, config, encoding).await?;
    Ok(true)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = WatermarkConfig::default();
        assert_eq!(config.offset_x, 20);
        assert_eq!(config.offset_y, 20);
        assert!((config.opacity - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_config_builder_pattern() {
        let config = WatermarkConfig::default()
            .with_offset(30, 40)
            .with_opacity(0.9);
        
        assert_eq!(config.offset_x, 30);
        assert_eq!(config.offset_y, 40);
        assert!((config.opacity - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_opacity_clamping() {
        let config = WatermarkConfig::default().with_opacity(1.5);
        assert!((config.opacity - 1.0).abs() < 0.01);
        
        let config = WatermarkConfig::default().with_opacity(-0.5);
        assert!((config.opacity - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_overlay_filter_with_opacity() {
        let config = WatermarkConfig::default();
        let filter = build_overlay_filter(&config);
        assert!(filter.contains("colorchannelmixer"));
        assert!(filter.contains("aa=0.70"));
    }

    #[test]
    fn test_overlay_filter_full_opacity() {
        let config = WatermarkConfig::default().with_opacity(1.0);
        let filter = build_overlay_filter(&config);
        assert!(!filter.contains("colorchannelmixer"));
    }

    #[test]
    fn test_is_available_false_for_missing() {
        let config = WatermarkConfig::default()
            .with_image_path("/nonexistent/path.png");
        assert!(!config.is_available());
    }
}
