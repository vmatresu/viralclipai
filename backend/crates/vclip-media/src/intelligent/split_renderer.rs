//! Split Renderer - Extracted split rendering logic.
//!
//! This module contains the rendering logic for split-view styles,
//! extracted from tier_aware_split.rs for better modularity.

use std::path::Path;
use std::process::Stdio;
use tracing::info;
use vclip_models::EncodingConfig;

use super::detection_adapter::SplitLayoutInfo;
use super::models::BoundingBox;
use super::output_format::{clamp_crop_to_frame, SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH};
use crate::error::{MediaError, MediaResult};
use crate::watermark::{append_watermark_filter_complex, WatermarkConfig};

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
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let center_x = width as f64 / 2.0;
    let half_width = center_x;
    let panel_ratio = SPLIT_PANEL_WIDTH as f64 / SPLIT_PANEL_HEIGHT as f64; // 9:8 = 1.125

    // Compute UNIFORM crop dimensions for both panels based on frame size.
    // This ensures consistent sizing and that crops are large enough to capture
    // faces regardless of their position in the frame.
    //
    // Use 85% of frame height as the crop height, then compute width from that.
    // This gives room for faces anywhere in the vertical frame.
    let min_crop_height = height as f64 * 0.85;
    let min_crop_width_from_height = min_crop_height * panel_ratio;

    // Also compute based on half-width (each speaker is in one half)
    let max_crop_width_from_half = half_width * 0.95;
    let crop_height_from_half = max_crop_width_from_half / panel_ratio;

    // Use the smaller of the two to ensure we fit within both constraints
    let (uniform_crop_width, uniform_crop_height) =
        if min_crop_width_from_height <= max_crop_width_from_half {
            (min_crop_width_from_height, min_crop_height)
        } else {
            (max_crop_width_from_half, crop_height_from_half)
        };

    // Use uniform dimensions for both panels
    let crop_width_left = uniform_crop_width;
    let crop_width_right = uniform_crop_width;
    let tile_height_left = uniform_crop_height.min(height as f64);
    let tile_height_right = uniform_crop_height.min(height as f64);

    let left_cx = left_box.cx();
    let right_cx = right_box.cx();

    // Position crops horizontally centered on face positions
    let left_crop_x = (left_cx - crop_width_left / 2.0)
        .max(0.0)
        .min(center_x - crop_width_left * 0.1);
    let right_crop_x = (right_cx - crop_width_right / 2.0)
        .max(center_x)
        .min(width as f64 - crop_width_right);

    let vertical_margin_left = height as f64 - tile_height_left;
    let vertical_margin_right = height as f64 - tile_height_right;

    // Compute bias to ensure face is fully contained in crop with headroom.
    // Key principle: never cut faces - the crop must contain the full head.
    let compute_safe_bias = |face_box: &BoundingBox, margin: f64, crop_h: f64| -> f64 {
        if margin <= 0.0 {
            return 0.0; // No margin means crop fills frame, bias doesn't matter
        }

        let face_top = face_box.y;
        let face_height = face_box.height;
        let face_bottom = face_top + face_height;

        // Required headroom: 60% of face height above face top for full head/scalp
        let headroom = face_height * 0.60;
        let head_top = (face_top - headroom).max(0.0);

        // Required footroom: 20% below chin for natural framing
        let footroom = face_height * 0.20;
        let head_bottom = face_bottom + footroom;

        // Compute valid crop_y range that contains the full head:
        // - crop_y <= head_top (to include top of head)
        // - crop_y + crop_h >= head_bottom (to include chin)
        let crop_y_max = head_top; // Upper bound: don't cut head top
        let crop_y_min = (head_bottom - crop_h).max(0.0); // Lower bound: don't cut chin

        // Convert to bias (crop_y = margin * bias)
        let bias_for_max = crop_y_max / margin;
        let bias_for_min = crop_y_min / margin;

        // Choose bias in valid range, preferring to show more headroom (lower bias)
        if bias_for_min <= bias_for_max {
            // Valid range exists - use aesthetic positioning within range
            let face_center_y = face_box.cy();
            let target_ratio = 0.40; // Place face at 40% from top of crop
            let ideal_crop_y = face_center_y - crop_h * target_ratio;
            let ideal_bias = (ideal_crop_y / margin).max(0.0);

            // Clamp to valid range
            ideal_bias.clamp(bias_for_min, bias_for_max).clamp(0.0, 0.8)
        } else {
            // Face is too large for crop - use middle of constraints
            ((bias_for_min + bias_for_max) / 2.0).clamp(0.0, 0.8)
        }
    };

    let left_bias = compute_safe_bias(left_box, vertical_margin_left, tile_height_left);
    let right_bias = compute_safe_bias(right_box, vertical_margin_right, tile_height_right);

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
        "[SPEAKER_SPLIT] Frame: {}x{}, uniform crop: {}x{}",
        width, height, uniform_crop_width as i32, uniform_crop_height as i32
    );
    info!(
        "[SPEAKER_SPLIT] Left face: y={:.0} h={:.0}, bias={:.2}, crop {}x{} at ({}, {})",
        left_box.y,
        left_box.height,
        left_bias,
        crop_width_left_u32,
        tile_height_left_u32,
        left_crop_x,
        left_crop_y
    );
    info!(
        "[SPEAKER_SPLIT] Right face: y={:.0} h={:.0}, bias={:.2}, crop {}x{} at ({}, {})",
        right_box.y,
        right_box.height,
        right_bias,
        crop_width_right_u32,
        tile_height_right_u32,
        right_crop_x,
        right_crop_y
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

    let base_filter_complex = format!(
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

    let (filter_complex, map_label) = if let Some(config) = watermark {
        if let Some(watermarked) =
            append_watermark_filter_complex(&base_filter_complex, "vout", config)
        {
            (watermarked.filter_complex, watermarked.output_label)
        } else {
            (base_filter_complex, "vout".to_string())
        }
    } else {
        (base_filter_complex, "vout".to_string())
    };

    run_ffmpeg_split(segment, output, &filter_complex, encoding, &map_label).await
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
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let left_bias = split_info.left_vertical_bias(height);
    let right_bias = split_info.right_vertical_bias(height);
    let left_horizontal = split_info.left_horizontal_center(width);
    let right_horizontal = split_info.right_horizontal_center(width);

    info!(
        "[STANDARD_SPLIT] Rendering with biases: L_vert={:.2}, R_vert={:.2}, L_horz={:.2}, R_horz={:.2}",
        left_bias, right_bias, left_horizontal, right_horizontal
    );

    // Use SinglePassRenderer for standard split
    let mut renderer = super::single_pass_renderer::SinglePassRenderer::new(
        super::config::IntelligentCropConfig::default(),
    );
    if let Some(config) = watermark {
        renderer = renderer.with_watermark(config.clone());
    }
    renderer
        .render_split(
            segment,
            output,
            width,
            height,
            left_bias,
            right_bias,
            left_horizontal,
            right_horizontal,
            encoding,
        )
        .await
}

/// Run FFmpeg with a filter complex for split rendering.
async fn run_ffmpeg_split(
    segment: &Path,
    output: &Path,
    filter_complex: &str,
    encoding: &EncodingConfig,
    map_label: &str,
) -> MediaResult<()> {
    let mut cmd = crate::command::create_ffmpeg_command();
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
        &format!("[{}]", map_label),
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
