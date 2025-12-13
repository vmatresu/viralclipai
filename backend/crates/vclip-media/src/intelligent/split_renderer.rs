//! Split Renderer - Extracted split rendering logic.
//!
//! This module contains the rendering logic for split-view styles,
//! extracted from tier_aware_split.rs for better modularity.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;
use vclip_models::EncodingConfig;

use super::detection_adapter::SplitLayoutInfo;
use super::models::BoundingBox;
use super::output_format::{clamp_crop_to_frame, SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH};
use crate::error::{MediaError, MediaResult};

/// Render a speaker-aware split view using custom crop boxes.
///
/// This function creates a single-pass FFmpeg command that:
/// 1. Crops left and right regions centered on detected speakers
/// 2. Scales each to panel dimensions
/// 3. Stacks them vertically
///
/// # Arguments
/// * `segment` - Input video segment
/// * `output` - Output path
/// * `width` - Video width
/// * `height` - Video height
/// * `left_box` - Bounding box for left speaker
/// * `right_box` - Bounding box for right speaker
/// * `encoding` - Encoding configuration
pub async fn render_speaker_split(
    segment: &Path,
    output: &Path,
    width: u32,
    height: u32,
    left_box: &BoundingBox,
    right_box: &BoundingBox,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    let center_x = width as f64 / 2.0;

    // Width tuned to keep single speaker per panel
    let crop_width_left = left_box
        .width
        .min(width as f64 * 0.55)
        .max(width as f64 * 0.25);
    let crop_width_right = right_box
        .width
        .min(width as f64 * 0.55)
        .max(width as f64 * 0.25);

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

    let left_bias = (left_box.cy() / height as f64 - 0.3).clamp(0.0, 0.4);
    let right_bias = (right_box.cy() / height as f64 - 0.3).clamp(0.0, 0.4);

    let left_crop_y = (vertical_margin_left * left_bias).round();
    let right_crop_y = (vertical_margin_right * right_bias).round();

    // Clamp crop coordinates
    let (left_crop_x, left_crop_y, crop_width_left_u32, tile_height_left_u32) = clamp_crop_to_frame(
        left_crop_x as i32,
        left_crop_y as i32,
        crop_width_left as i32,
        tile_height_left as i32,
        width,
        height,
    );
    let (right_crop_x, right_crop_y, crop_width_right_u32, tile_height_right_u32) =
        clamp_crop_to_frame(
            right_crop_x as i32,
            right_crop_y as i32,
            crop_width_right as i32,
            tile_height_right as i32,
            width,
            height,
        );

    info!(
        "[SPEAKER_SPLIT] Left: crop {}x{} at ({}, {})",
        crop_width_left_u32, tile_height_left_u32, left_crop_x, left_crop_y
    );
    info!(
        "[SPEAKER_SPLIT] Right: crop {}x{} at ({}, {})",
        crop_width_right_u32, tile_height_right_u32, right_crop_x, right_crop_y
    );

    // Adjust crop dimensions to exactly match 9:8 panel aspect ratio (zoom to fill)
    let panel_ratio = SPLIT_PANEL_WIDTH as f64 / SPLIT_PANEL_HEIGHT as f64; // 9:8 = 1.125

    // Left panel: adjust to 9:8
    let left_source_ratio = crop_width_left_u32 as f64 / tile_height_left_u32 as f64;
    let (final_left_w, final_left_h, left_x_adj, left_y_adj) = if left_source_ratio > panel_ratio {
        // Source is wider - crop width
        let h = tile_height_left_u32;
        let w = (h as f64 * panel_ratio).round() as i32;
        let x_adj = (crop_width_left_u32 as i32 - w) / 2;
        (w, h as i32, x_adj, 0)
    } else {
        // Source is taller - crop height
        let w = crop_width_left_u32;
        let h = (w as f64 / panel_ratio).round() as i32;
        let y_adj = (tile_height_left_u32 as i32 - h) / 2;
        (w as i32, h, 0, y_adj)
    };

    // Right panel: adjust to 9:8
    let right_source_ratio = crop_width_right_u32 as f64 / tile_height_right_u32 as f64;
    let (final_right_w, final_right_h, right_x_adj, right_y_adj) =
        if right_source_ratio > panel_ratio {
            let h = tile_height_right_u32;
            let w = (h as f64 * panel_ratio).round() as i32;
            let x_adj = (crop_width_right_u32 as i32 - w) / 2;
            (w, h as i32, x_adj, 0)
        } else {
            let w = crop_width_right_u32;
            let h = (w as f64 / panel_ratio).round() as i32;
            let y_adj = (tile_height_right_u32 as i32 - h) / 2;
            (w as i32, h, 0, y_adj)
        };

    let filter_complex = format!(
        "[0:v]split=2[left_in][right_in];\
         [left_in]crop={lw}:{lth}:{lx}:{ly},scale={pw}:{ph}:flags=lanczos,setsar=1,format=yuv420p[top];\
         [right_in]crop={rw}:{rth}:{rx}:{ry},scale={pw}:{ph}:flags=lanczos,setsar=1,format=yuv420p[bottom];\
         [top][bottom]vstack=inputs=2[vout]",
        lw = final_left_w,
        lx = left_crop_x + left_x_adj,
        lth = final_left_h,
        ly = left_crop_y + left_y_adj,
        rw = final_right_w,
        rx = right_crop_x + right_x_adj,
        rth = final_right_h,
        ry = right_crop_y + right_y_adj,
        pw = SPLIT_PANEL_WIDTH,
        ph = SPLIT_PANEL_HEIGHT,
    );

    run_ffmpeg_split(segment, output, &filter_complex, encoding).await
}

/// Render a standard split view with vertical bias positioning.
///
/// # Arguments
/// * `segment` - Input video segment
/// * `output` - Output path
/// * `width` - Video width
/// * `height` - Video height
/// * `split_info` - Split layout information with face positions
/// * `encoding` - Encoding configuration
pub async fn render_standard_split(
    segment: &Path,
    output: &Path,
    width: u32,
    height: u32,
    split_info: &SplitLayoutInfo,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    let left_bias = split_info.left_vertical_bias(height);
    let right_bias = split_info.right_vertical_bias(height);

    info!(
        "[STANDARD_SPLIT] Rendering with biases: left={:.2}, right={:.2}",
        left_bias, right_bias
    );

    // Use SinglePassRenderer for standard split
    let renderer = super::single_pass_renderer::SinglePassRenderer::new(
        super::config::IntelligentCropConfig::default(),
    );
    renderer
        .render_split(
            segment, output, width, height, left_bias, right_bias, encoding,
        )
        .await
}

/// Run FFmpeg with a filter complex for split rendering.
async fn run_ffmpeg_split(
    segment: &Path,
    output: &Path,
    filter_complex: &str,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        segment.to_str().unwrap_or(""),
        "-filter_complex",
        filter_complex,
        "-map",
        "[vout]",
        "-map",
        "0:a?",
        "-c:v",
        &encoding.codec,
        "-preset",
        &encoding.preset,
        "-crf",
        &encoding.crf.to_string(),
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        &encoding.audio_bitrate,
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
            "Split render failed",
            Some(stderr.to_string()),
            result.status.code(),
        ));
    }

    info!("[SPLIT_RENDERER] Single-pass encoding complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_clamping() {
        let (x, y, w, h) = clamp_crop_to_frame(-10, -10, 100, 100, 1920, 1080);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert!(w <= 1920);
        assert!(h <= 1080);
    }
}
