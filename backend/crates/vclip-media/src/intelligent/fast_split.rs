//! Fast split engine for heuristic-based video splitting.
//!
//! This module extracts the fast/deterministic split algorithm that doesn't
//! rely on AI face detection. It splits landscape videos into left/right
//! halves and stacks them vertically using fixed geometric positioning.
//!
//! # Algorithm (SINGLE-PASS)
//!
//! Uses ONE FFmpeg command with a combined filter graph:
//! ```text
//! [0:v] → split → [left][right]
//! [left] → crop left 45% → scale 1080x960 → [top]
//! [right] → crop right 45% → scale 1080x960 → [bottom]
//! [top][bottom] → vstack → [out]
//! ```
//!
//! This avoids multiple encode passes for better quality and smaller files.

use std::path::Path;
use tracing::info;

use super::single_pass_renderer::SinglePassRenderer;
use super::config::IntelligentCropConfig;
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
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
        let start_time = std::time::Instant::now();

        info!("[FAST_SPLIT] ========================================");
        info!("[FAST_SPLIT] START: {:?}", segment);

        // 1. Get video metadata
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;

        info!(
            "[FAST_SPLIT] Source: {}x{} @ {:.2}fps, {:.2}s",
            width, height, video_info.fps, video_info.duration
        );

        // 2. Use SinglePassRenderer with fixed positioning (SINGLE ENCODE)
        info!("[FAST_SPLIT] Processing with SINGLE-PASS encoding...");
        info!("[FAST_SPLIT]   Encoding: {} preset={} crf={}", 
            encoding.codec, encoding.preset, encoding.crf);
        
        let config = IntelligentCropConfig::default();
        let renderer = SinglePassRenderer::new(config);
        
        renderer.render_split(
            segment,
            output,
            width,
            height,
            self.config.top_vertical_bias,
            self.config.bottom_vertical_bias,
            encoding,
        ).await?;

        // 3. Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("[FAST_SPLIT] Failed to generate thumbnail: {}", e);
        }

        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!("[FAST_SPLIT] ========================================");
        info!(
            "[FAST_SPLIT] COMPLETE in {:.2}s - {:.2} MB",
            start_time.elapsed().as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
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
