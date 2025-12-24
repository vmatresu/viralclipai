//! Streamer Split style processor.
//!
//! Creates a split view optimized for gaming/streaming content:
//! - Top panel: User-specified webcam crop region (scaled to 9:8)
//! - Bottom panel: Center-cropped gaming content (no black bars, 9:8)
//!
//! Audio comes only from the original video.
//!
//! This style is FREE (no AI detection required) and uses user-specified
//! parameters for the top panel webcam position and zoom level.

use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tracing::{info, warn};
use vclip_models::{DetectionTier, EncodingConfig, StreamerSplitParams, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::{MediaError, MediaResult};
use crate::intelligent::output_format::{SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH};
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use crate::watermark::{append_watermark_filter_complex, WatermarkConfig};

use super::utils;

/// Panel dimensions (each panel is 9:8 aspect ratio)
const PANEL_WIDTH: u32 = SPLIT_PANEL_WIDTH;
const PANEL_HEIGHT: u32 = SPLIT_PANEL_HEIGHT;

/// Processor for streamer split video style.
///
/// Creates a split view with:
/// - User-specified webcam region on top (scaled to 9:8)
/// - Center-cropped gaming content on bottom (no black bars, 9:8)
#[derive(Clone, Default)]
pub struct StreamerSplitProcessor;

impl StreamerSplitProcessor {
    /// Create a new streamer split processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the detection tier (None - no AI detection needed).
    pub fn detection_tier(&self) -> DetectionTier {
        DetectionTier::None
    }
}

#[async_trait]
impl StyleProcessor for StreamerSplitProcessor {
    fn name(&self) -> &'static str {
        "streamer_split"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::StreamerSplit)
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        ctx.security.check_resource_limits("ffmpeg")?;
        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        let timer = ctx.metrics.start_timer("streamer_split_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            "streamer_split".to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        // Get user-specified params or use defaults
        let params = request
            .task
            .streamer_split_params
            .clone()
            .unwrap_or_default();

        info!(
            "[STREAMER_SPLIT] Processing with user params: pos=({:?}, {:?}), zoom={:.1}",
            params.position_x, params.position_y, params.zoom
        );

        // Process the streamer split with user params
        process_streamer_split(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            &request.encoding,
            &params,
            request.watermark.as_ref(),
        )
        .await?;

        let processing_time = timer.elapsed();

        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration = crate::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0)
            - crate::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(utils::thumbnail_path(&request.output_path).into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics
            .increment_counter("processing_completed", &[("style", "streamer_split")]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", "streamer_split")],
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> crate::core::ProcessingComplexity {
        let duration = crate::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0)
            - crate::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);

        // StreamerSplit is now fast (no AI detection), so reduce complexity estimate
        utils::estimate_complexity(duration, false)
    }
}

/// Process a video into streamer split format.
///
/// Creates a 9:16 output with:
/// - Top panel (9:8): User-specified webcam region
/// - Bottom panel (9:8): Center-cropped gaming content (no black bars)
async fn process_streamer_split(
    input: &Path,
    output: &Path,
    task: &vclip_models::ClipTask,
    encoding: &EncodingConfig,
    params: &StreamerSplitParams,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let pipeline_start = std::time::Instant::now();

    info!("[STREAMER_SPLIT] ========================================");
    info!("[STREAMER_SPLIT] START: {:?}", input);

    // Step 1: Probe video
    let video_info = probe_video(input).await?;
    let width = video_info.width;
    let height = video_info.height;

    info!(
        "[STREAMER_SPLIT] Video: {}x{} @ {:.2}fps, {:.2}s",
        width, height, video_info.fps, video_info.duration
    );

    // Step 2: Extract segment with padding
    let start_secs = (crate::intelligent::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = crate::intelligent::parse_timestamp(&task.end)? + task.pad_after;
    let clip_duration = end_secs - start_secs;

    let segment_path = output.with_extension("segment.mp4");
    crate::clip::extract_segment(input, &segment_path, start_secs, clip_duration).await?;

    // Re-probe segment for accurate dimensions
    let segment_info = probe_video(&segment_path).await?;
    let seg_width = segment_info.width;
    let seg_height = segment_info.height;

    // Step 3: Compute crop region from user params
    let crop_region = compute_crop_from_params(params, seg_width, seg_height);

    info!(
        "[STREAMER_SPLIT] Crop region: {}x{} at ({}, {}), zoom: {:.1}x",
        crop_region.width, crop_region.height, crop_region.x, crop_region.y, params.zoom
    );

    // Step 4: Render the streamer split
    info!("[STREAMER_SPLIT] Rendering split view...");

    render_streamer_split(&segment_path, output, &crop_region, encoding, params, watermark).await?;

    // Cleanup segment
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            warn!("[STREAMER_SPLIT] Failed to cleanup segment: {}", e);
        }
    }

    // Generate thumbnail
    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        warn!("[STREAMER_SPLIT] Failed to generate thumbnail: {}", e);
    }

    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!("[STREAMER_SPLIT] ========================================");
    info!(
        "[STREAMER_SPLIT] COMPLETE in {:.2}s - {:.2} MB",
        pipeline_start.elapsed().as_secs_f64(),
        file_size as f64 / 1_000_000.0
    );

    Ok(())
}

/// Crop region for the top panel (webcam area).
#[derive(Debug, Clone, Copy)]
struct CropRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Compute crop region from user-specified parameters.
///
/// The crop region is calculated based on:
/// - Manual crop (if provided)
/// - OR Position (horizontal: left/center/right, vertical: top/middle/bottom)
/// - Zoom level (1.0 = full frame, 2.0 = 2x zoom, etc.)
fn compute_crop_from_params(params: &StreamerSplitParams, width: u32, height: u32) -> CropRegion {
    // Priority: Manual crop > Position presets
    if let Some(rect) = params.manual_crop {
        // Clamp to 0.0-1.0 to be safe
        let nx = rect.x.clamp(0.0, 1.0);
        let ny = rect.y.clamp(0.0, 1.0);
        let nw = rect.width.clamp(0.0, 1.0);
        let nh = rect.height.clamp(0.0, 1.0);

        let x = (nx * width as f64).round() as u32;
        let y = (ny * height as f64).round() as u32;
        let w = (nw * width as f64).round() as u32;
        let h = (nh * height as f64).round() as u32;

        // Ensure valid dimensions
        let w = w.max(16).min(width - x);
        let h = h.max(16).min(height - y);

        return CropRegion {
            x,
            y,
            width: w,
            height: h,
        };
    }

    let panel_ratio = PANEL_WIDTH as f64 / PANEL_HEIGHT as f64; // 9:8 = 1.125

    // Calculate crop size based on zoom level
    // zoom = 1.0 means full frame, zoom = 2.0 means half the frame, etc.
    let zoom = params.zoom.clamp(1.0, 4.0) as f64;
    let crop_width = (width as f64 / zoom).round() as u32;
    let crop_height = (crop_width as f64 / panel_ratio).round() as u32;

    // Clamp to frame bounds
    let crop_width = crop_width.min(width);
    let crop_height = crop_height.min(height);

    // Calculate position based on user selection
    let norm_x = params.position_x.to_normalized();
    let norm_y = params.position_y.to_normalized();

    // Convert normalized position to pixel coordinates
    // The position represents where the CENTER of the crop should be
    let center_x = (norm_x * width as f64).round() as i32;
    let center_y = (norm_y * height as f64).round() as i32;

    // Calculate top-left corner, clamping to frame bounds
    let x = (center_x - crop_width as i32 / 2).clamp(0, (width - crop_width) as i32) as u32;
    let y = (center_y - crop_height as i32 / 2).clamp(0, (height - crop_height) as i32) as u32;

    CropRegion {
        x,
        y,
        width: crop_width,
        height: crop_height,
    }
}

/// Render the streamer split video.
///
/// Creates a single-pass FFmpeg command that:
/// 1. Crops and scales user-specified webcam region for top panel (9:8)
/// 2. Center-crops the gaming content for bottom panel (9:8, no black bars)
/// 3. Stacks them vertically
/// 4. Uses audio only from the original (input)
async fn render_streamer_split(
    segment: &Path,
    output: &Path,
    crop: &CropRegion,
    encoding: &EncodingConfig,
    params: &StreamerSplitParams,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    // Calculate split dimensions
    // Default 50/50 split (0.5), range 0.1 to 0.9
    let ratio = params.split_ratio.unwrap_or(0.5).clamp(0.1, 0.9);
    
    // Total height is 1920 (PORTRAIT_HEIGHT)
    // We must ensure heights are even for libx264
    let total_height = crate::intelligent::output_format::PORTRAIT_HEIGHT;
    let top_height = crate::intelligent::output_format::make_even((total_height as f32 * ratio) as i32) as u32;


    // Build filter complex:
    // - Top panel: webcam region scaled preserving aspect ratio (no stretching)
    // - Bottom panel: Center crop scaled to fill remaining space
    // - Use overlay so bottom panel covers any black bar from webcam padding
    
    // Calculate actual webcam scaled height (preserving aspect ratio)
    // The webcam crop has aspect ratio crop.width / crop.height
    // When scaled to PANEL_WIDTH, the height will be:
    // scaled_height = PANEL_WIDTH * crop.height / crop.width
    let webcam_aspect = crop.width as f64 / crop.height as f64;
    let scaled_webcam_height = (PANEL_WIDTH as f64 / webcam_aspect).round() as u32;
    // Clamp to top_height max (in case webcam is very wide)
    let actual_webcam_height = scaled_webcam_height.min(top_height);
    
    // Bottom panel starts where webcam ends (no gap)
    // Bottom panel height = total - actual_webcam_height
    let actual_bottom_height = total_height - actual_webcam_height;
    
    // Ensure even heights for libx264
    let actual_webcam_height = crate::intelligent::output_format::make_even(actual_webcam_height as i32) as u32;
    let actual_bottom_height = crate::intelligent::output_format::make_even(actual_bottom_height as i32) as u32;
    
    let base_filter_complex = format!(
        "[0:v]crop={cw}:{ch}:{cx}:{cy},\
         scale={pw}:{wh}:flags=lanczos,\
         setsar=1,format=yuv420p[top];\
         [0:v]crop=ih*{pw}/{bh}:ih:(iw-ih*{pw}/{bh})/2:0,\
         scale={pw}:{bh}:flags=lanczos,\
         setsar=1,format=yuv420p[bottom];\
         [top][bottom]vstack=inputs=2[vout]",
        pw = PANEL_WIDTH,
        wh = actual_webcam_height,
        bh = actual_bottom_height,
        cw = crop.width,
        ch = crop.height,
        cx = crop.x,
        cy = crop.y,
    );
    let (filter_complex, map_label) = if let Some(config) = watermark {
        if let Some(watermarked) = append_watermark_filter_complex(&base_filter_complex, "vout", config) {
            (watermarked.filter_complex, watermarked.output_label)
        } else {
            (base_filter_complex, "vout".to_string())
        }
    } else {
        (base_filter_complex, "vout".to_string())
    };

    let mut cmd = crate::command::create_ffmpeg_command();
    cmd.args([
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        segment.to_str().unwrap_or(""),
        "-filter_complex",
        &filter_complex,
        "-map",
        &format!("[{}]", map_label),
        "-map",
        "0:a?", // Audio from original only
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
            "Streamer split render failed",
            Some(stderr.to_string()),
            result.status.code(),
        ));
    }

    info!("[STREAMER_SPLIT] Single-pass encoding complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::{HorizontalPosition, VerticalPosition};

    #[test]
    fn test_streamer_split_processor_creation() {
        let processor = StreamerSplitProcessor::new();
        assert_eq!(processor.name(), "streamer_split");
        assert!(processor.can_handle(Style::StreamerSplit));
        assert!(!processor.can_handle(Style::Split));
        // Now uses DetectionTier::None (no AI detection)
        assert_eq!(processor.detection_tier(), DetectionTier::None);
    }

    #[test]
    fn test_crop_region_top_left() {
        let params = StreamerSplitParams {
            position_x: HorizontalPosition::Left,
            position_y: VerticalPosition::Top,
            zoom: 2.0,
            static_image_url: None,
            manual_crop: None,
            split_ratio: None,
        };
        let crop = compute_crop_from_params(&params, 1920, 1080);
        
        // With 2x zoom, crop should be half the frame width
        assert_eq!(crop.width, 960);
        // Crop should be at top-left
        assert_eq!(crop.x, 0);
        assert_eq!(crop.y, 0);
    }

    #[test]
    fn test_crop_region_center() {
        let params = StreamerSplitParams {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 1.0,
            static_image_url: None,
            manual_crop: None,
            split_ratio: None,
        };
        let crop = compute_crop_from_params(&params, 1920, 1080);
        
        // With 1x zoom, crop should be full frame width
        assert_eq!(crop.width, 1920);
        // Crop should be at origin (full frame)
        assert_eq!(crop.x, 0);
    }

    #[test]
    fn test_crop_region_bottom_right() {
        let params = StreamerSplitParams {
            position_x: HorizontalPosition::Right,
            position_y: VerticalPosition::Bottom,
            zoom: 2.0,
            static_image_url: None,
            manual_crop: None,
            split_ratio: None,
        };
        let crop = compute_crop_from_params(&params, 1920, 1080);
        
        // With 2x zoom, crop should be half the frame width
        assert_eq!(crop.width, 960);
        // Crop should be at bottom-right
        assert_eq!(crop.x, 960); // 1920 - 960
    }
        #[test]
    fn test_manual_crop_priority() {
        let params = StreamerSplitParams {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 1.0,
            static_image_url: None,
            manual_crop: Some(vclip_models::NormalizedRect {
                x: 0.1,
                y: 0.1,
                width: 0.5,
                height: 0.5,
            }),
            split_ratio: None,
        };
        let crop = compute_crop_from_params(&params, 1000, 1000);
        
        // Should use manual crop (0.1 start, 0.5 width = 100px x, 500px w)
        assert_eq!(crop.x, 100);
        assert_eq!(crop.y, 100);
        assert_eq!(crop.width, 500);
        assert_eq!(crop.height, 500);
    }

    #[test]
    fn test_zoom_clamping() {
        // Test that zoom is clamped to valid range
        let params_low = StreamerSplitParams {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 0.5, // Below minimum
            static_image_url: None,
            manual_crop: None,
            split_ratio: None,
        };
        let crop_low = compute_crop_from_params(&params_low, 1920, 1080);
        // Should be clamped to 1.0 (full frame)
        assert_eq!(crop_low.width, 1920);

        let params_high = StreamerSplitParams {
            position_x: HorizontalPosition::Center,
            position_y: VerticalPosition::Middle,
            zoom: 10.0, // Above maximum
            static_image_url: None,
            manual_crop: None,
            split_ratio: None,
        };
        let crop_high = compute_crop_from_params(&params_high, 1920, 1080);
        // Should be clamped to 4.0 (quarter frame)
        assert_eq!(crop_high.width, 480); // 1920 / 4
    }

    #[test]
    fn test_default_params() {
        let params = StreamerSplitParams::default();
        assert_eq!(params.position_x, HorizontalPosition::Left);
        assert_eq!(params.position_y, VerticalPosition::Top);
        assert!((params.zoom - 1.5).abs() < 0.01);
        assert!(params.static_image_url.is_none());
    }
}
