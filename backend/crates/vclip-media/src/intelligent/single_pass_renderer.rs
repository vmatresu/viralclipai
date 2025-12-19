//! Single-pass video rendering for intelligent cropping.
//!
//! This module provides **single-pass processing** that eliminates:
//! - Multiple encode passes
//! - Generation loss artifacts (CRT lines, flickering)
//!
//! # Pipeline
//!
//! 1. `extract_segment()` - Stream copy from source (NO encode)
//! 2. Face detection on segment
//! 3. `SinglePassRenderer` - ONE encode with all filters combined
//!
//! # Why Single Encode?
//!
//! Previous pipeline had 2-4 encode passes causing:
//! - Huge file sizes (13MB for 30s instead of ~4MB)
//! - CRT scan-line artifacts on scene changes
//! - Quality degradation
//!
//! Single encode = one decode + one encode = best quality.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use super::config::IntelligentCropConfig;
use super::models::CropWindow;
use super::output_format::{
    PORTRAIT_HEIGHT, PORTRAIT_WIDTH, SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH,
};
use crate::error::{MediaError, MediaResult};
use crate::watermark::{append_watermark_filter_complex, build_vf_with_watermark, WatermarkConfig};
use vclip_models::EncodingConfig;

/// Single-pass renderer that applies all transforms in ONE encode.
pub struct SinglePassRenderer {
    #[allow(dead_code)]
    config: IntelligentCropConfig,
    watermark: Option<WatermarkConfig>,
}

impl SinglePassRenderer {
    /// Create a new single-pass renderer.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self {
            config,
            watermark: None,
        }
    }

    pub fn with_watermark(mut self, config: WatermarkConfig) -> Self {
        self.watermark = Some(config);
        self
    }

    /// Render intelligent full-frame crop in a single encode pass.
    ///
    /// Input should be a **pre-extracted segment** (stream copy from source).
    /// This function does the ONE and ONLY encode in the pipeline.
    ///
    /// # Arguments
    /// * `segment` - Pre-extracted segment file (stream copy, not re-encoded)
    /// * `output` - Final output path
    /// * `crop_windows` - Computed crop windows from face detection
    /// * `encoding` - Encoding configuration from API
    pub async fn render_full<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        crop_windows: &[CropWindow],
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();
        let start_time = std::time::Instant::now();

        info!(
            "[RENDER_FULL] START: {} -> {}",
            segment.display(),
            output.display()
        );

        if crop_windows.is_empty() {
            return Err(MediaError::InvalidVideo(
                "No crop windows provided".to_string(),
            ));
        }

        // Use median crop for static rendering (most common case)
        let crop = Self::compute_median_crop(crop_windows);

        info!(
            "[RENDER_FULL] Crop: {}x{} at ({}, {})",
            crop.width, crop.height, crop.x, crop.y
        );
        info!(
            "[RENDER_FULL] Encoding: {} preset={} crf={}",
            encoding.codec, encoding.preset, encoding.crf
        );

        // Build filter: crop → scale to exact output dimensions → set SAR
        // The crop window is computed with exact 9:16 aspect ratio (zoom-to-fill),
        // so we can scale directly without padding - no black bars, no stretching
        let base_filter = format!(
            "crop={}:{}:{}:{},scale={}:{}:flags=lanczos,setsar=1",
            crop.width, crop.height, crop.x, crop.y, PORTRAIT_WIDTH, PORTRAIT_HEIGHT
        );
        let filter = if let Some(config) = self.watermark.as_ref() {
            build_vf_with_watermark(Some(&base_filter), config).unwrap_or(base_filter)
        } else {
            base_filter
        };

        // Single FFmpeg command - THE ONLY ENCODE
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            // Input is the pre-extracted segment
            "-i",
            segment.to_str().unwrap_or(""),
            // Video filter
            "-vf",
            &filter,
            // Video encoding - SINGLE ENCODE using API config
            "-c:v",
            &encoding.codec,
            "-preset",
            &encoding.preset,
            "-crf",
            &encoding.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            // Audio encoding
            "-c:a",
            "aac",
            "-b:a",
            &encoding.audio_bitrate,
            // Output options
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        debug!("FFmpeg command: {:?}", cmd);

        let result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "Single-pass render failed",
                Some(stderr.to_string()),
                result.status.code(),
            ));
        }

        let elapsed = start_time.elapsed();
        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!(
            "[RENDER_FULL] DONE in {:.2}s - output: {:.2} MB",
            elapsed.as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
    }

    /// Render split view in a single encode pass.
    ///
    /// Input should be a **pre-extracted segment** (stream copy from source).
    /// Combines left/right crop, scale, and vstack in ONE FFmpeg command.
    ///
    /// # Filter Graph
    /// ```text
    /// [0:v] → split → [left_in][right_in]
    /// [left_in] → crop left → scale 1080x960 → [top]
    /// [right_in] → crop right → scale 1080x960 → [bottom]
    /// [top][bottom] → vstack → [out]
    /// ```
    ///
    /// # Face Centering
    /// Uses horizontal and vertical bias to center the crop on detected faces.
    pub async fn render_split<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        source_width: u32,
        source_height: u32,
        left_vertical_bias: f64,
        right_vertical_bias: f64,
        left_horizontal_center: f64,
        right_horizontal_center: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();
        let start_time = std::time::Instant::now();

        info!(
            "[RENDER_SPLIT] START: {} -> {}",
            segment.display(),
            output.display()
        );

        let half_width = source_width / 2;

        // Calculate 9:8 tile dimensions (each panel is 1080x960)
        let panel_ratio = SPLIT_PANEL_WIDTH as f64 / SPLIT_PANEL_HEIGHT as f64; // 9:8 = 1.125

        // Compute crop dimensions for each panel
        // We need to fit a 9:8 crop within each half of the frame
        let max_crop_from_half = half_width;
        let ideal_crop_height = (max_crop_from_half as f64 / panel_ratio).round() as u32;
        let crop_height = ideal_crop_height.min(source_height);
        let crop_width = (crop_height as f64 * panel_ratio).round() as u32;

        // Vertical positioning based on bias
        let vertical_margin = source_height.saturating_sub(crop_height);
        let left_crop_y = (vertical_margin as f64 * left_vertical_bias).round() as u32;
        let right_crop_y = (vertical_margin as f64 * right_vertical_bias).round() as u32;

        // HORIZONTAL CENTERING on face positions
        // left_horizontal_center is 0-1 within left half (0.5 = center of left half)
        // right_horizontal_center is 0-1 within right half (0.5 = center of right half)

        // Left panel: face at left_horizontal_center * half_width
        let left_face_x = left_horizontal_center * half_width as f64;
        let left_crop_x = (left_face_x - crop_width as f64 / 2.0)
            .max(0.0)
            .min((half_width - crop_width) as f64) as u32;

        // Right panel: face at half_width + right_horizontal_center * half_width
        let right_face_x = half_width as f64 + right_horizontal_center * half_width as f64;
        let right_crop_x = (right_face_x - crop_width as f64 / 2.0)
            .max(half_width as f64)
            .min((source_width - crop_width) as f64) as u32;

        info!(
            "[RENDER_SPLIT] Source: {}x{}, Panel crop: {}x{}, Left: ({}, {}), Right: ({}, {})",
            source_width, source_height, crop_width, crop_height,
            left_crop_x, left_crop_y, right_crop_x, right_crop_y
        );
        info!(
            "[RENDER_SPLIT] Encoding: {} preset={} crf={}",
            encoding.codec, encoding.preset, encoding.crf
        );

        // Build combined filter graph - everything in ONE pass
        let base_filter_complex = format!(
            "[0:v]split=2[left_in][right_in];\
             [left_in]crop={cw}:{ch}:{lx}:{ly},scale={pw}:{ph}:flags=lanczos,setsar=1,format=yuv420p[top];\
             [right_in]crop={cw}:{ch}:{rx}:{ry},scale={pw}:{ph}:flags=lanczos,setsar=1,format=yuv420p[bottom];\
             [top][bottom]vstack=inputs=2[vout]",
            cw = crop_width,
            ch = crop_height,
            lx = left_crop_x,
            ly = left_crop_y,
            rx = right_crop_x,
            ry = right_crop_y,
            pw = SPLIT_PANEL_WIDTH,
            ph = SPLIT_PANEL_HEIGHT,
        );
        let (filter_complex, map_label) = if let Some(config) = self.watermark.as_ref() {
            if let Some(watermarked) = append_watermark_filter_complex(&base_filter_complex, "vout", config) {
                (watermarked.filter_complex, watermarked.output_label)
            } else {
                (base_filter_complex, "vout".to_string())
            }
        } else {
            (base_filter_complex, "vout".to_string())
        };

        debug!("Filter graph:\n{}", filter_complex);

        // Single FFmpeg command - THE ONLY ENCODE
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            // Input is the pre-extracted segment
            "-i",
            segment.to_str().unwrap_or(""),
            // Filter graph
            "-filter_complex",
            &filter_complex,
            // Map outputs
            "-map",
            &format!("[{}]", map_label),
            "-map",
            "0:a?",
            // Video encoding - SINGLE ENCODE using API config
            "-c:v",
            &encoding.codec,
            "-preset",
            &encoding.preset,
            "-crf",
            &encoding.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            // Audio
            "-c:a",
            "aac",
            "-b:a",
            &encoding.audio_bitrate,
            // Output
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "Single-pass split render failed",
                Some(stderr.to_string()),
                result.status.code(),
            ));
        }

        let elapsed = start_time.elapsed();
        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!(
            "[RENDER_SPLIT] DONE in {:.2}s - output: {:.2} MB",
            elapsed.as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
    }

    /// Compute median crop from windows for static rendering.
    fn compute_median_crop(windows: &[CropWindow]) -> CropWindow {
        if windows.is_empty() {
            return CropWindow::new(0.0, 0, 0, PORTRAIT_WIDTH as i32, PORTRAIT_HEIGHT as i32);
        }

        let mut x_vals: Vec<i32> = windows.iter().map(|w| w.x).collect();
        let mut y_vals: Vec<i32> = windows.iter().map(|w| w.y).collect();
        let mut w_vals: Vec<i32> = windows.iter().map(|w| w.width).collect();
        let mut h_vals: Vec<i32> = windows.iter().map(|w| w.height).collect();

        x_vals.sort();
        y_vals.sort();
        w_vals.sort();
        h_vals.sort();

        let mid = windows.len() / 2;

        CropWindow::new(
            windows[mid].time,
            x_vals[mid],
            y_vals[mid],
            w_vals[mid],
            h_vals[mid],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median_crop() {
        let windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 900),
            CropWindow::new(1.0, 200, 150, 600, 1000),
            CropWindow::new(2.0, 150, 125, 550, 950),
        ];

        let median = SinglePassRenderer::compute_median_crop(&windows);
        assert_eq!(median.x, 150);
        assert_eq!(median.y, 125);
        assert_eq!(median.width, 550);
        assert_eq!(median.height, 950);
    }

    #[test]
    fn test_empty_windows() {
        let windows: Vec<CropWindow> = vec![];
        let median = SinglePassRenderer::compute_median_crop(&windows);
        assert_eq!(median.width, 1080);
        assert_eq!(median.height, 1920);
    }
}
