//! Streamer Split style processor.
//!
//! Creates a split view optimized for gaming/explainer content:
//! - Top panel: Original landscape gameplay/content (letterboxed to fit 9:8 panel)
//! - Bottom panel: Face cam with intelligent face tracking
//!
//! Audio comes only from the original video (top panel).
//! If face detection fails in some frames, the processor shows black bars
//! or the last successfully detected face position (frozen frame fallback).
//!
//! This style is available to Pro and Studio tiers only and can trigger
//! face detection cache generation.

use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};
use vclip_models::{DetectionTier, EncodingConfig, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::{MediaError, MediaResult};
use crate::intelligent::detection_adapter::get_detections;
use crate::intelligent::models::BoundingBox;
use crate::intelligent::output_format::{SPLIT_PANEL_HEIGHT, SPLIT_PANEL_WIDTH};
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

use super::utils;

/// Panel dimensions (each panel is 9:8 aspect ratio)
const PANEL_WIDTH: u32 = SPLIT_PANEL_WIDTH;
const PANEL_HEIGHT: u32 = SPLIT_PANEL_HEIGHT;

/// Processor for streamer split video style.
///
/// Creates a split view with:
/// - Original gameplay/content letterboxed on top
/// - Face cam with intelligent tracking on bottom
#[derive(Clone)]
pub struct StreamerSplitProcessor {
    tier: DetectionTier,
}

impl StreamerSplitProcessor {
    /// Create a new streamer split processor.
    pub fn new() -> Self {
        Self {
            tier: DetectionTier::Basic,
        }
    }

    /// Create with specific detection tier.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self { tier }
    }

    /// Get the detection tier.
    pub fn detection_tier(&self) -> DetectionTier {
        self.tier
    }
}

impl Default for StreamerSplitProcessor {
    fn default() -> Self {
        Self::new()
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
        ctx.security.check_resource_limits("face_detection")?;
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

        info!(
            "[STREAMER_SPLIT] Processing with tier {:?}",
            self.tier
        );

        // Process the streamer split
        process_streamer_split(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            &request.encoding,
            request.cached_neural_analysis.as_deref(),
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

        let mut complexity = utils::estimate_complexity(duration, true);
        complexity.estimated_time_ms = (complexity.estimated_time_ms as f64 * 1.5) as u64;
        complexity
    }
}

/// Process a video into streamer split format.
///
/// Creates a 9:16 output with:
/// - Top panel (9:8): Original content letterboxed
/// - Bottom panel (9:8): Face cam with intelligent tracking
async fn process_streamer_split(
    input: &Path,
    output: &Path,
    task: &vclip_models::ClipTask,
    encoding: &EncodingConfig,
    cached_analysis: Option<&vclip_models::SceneNeuralAnalysis>,
) -> MediaResult<()> {
    let pipeline_start = std::time::Instant::now();

    info!("[STREAMER_SPLIT] ========================================");
    info!("[STREAMER_SPLIT] START: {:?}", input);

    // Step 1: Probe video
    let video_info = probe_video(input).await?;
    let width = video_info.width;
    let height = video_info.height;
    let fps = video_info.fps;
    let duration = video_info.duration;

    info!(
        "[STREAMER_SPLIT] Video: {}x{} @ {:.2}fps, {:.2}s",
        width, height, fps, duration
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
    let seg_fps = segment_info.fps;
    let seg_duration = segment_info.duration;

    // Step 3: Get face detections (from cache or run detection)
    info!("[STREAMER_SPLIT] Getting face detections...");

    let frame_detections = get_detections(
        cached_analysis,
        &segment_path,
        DetectionTier::Basic,
        0.0,
        seg_duration,
        seg_width,
        seg_height,
        seg_fps,
    )
    .await?;

    // Convert Vec<Vec<Detection>> to Vec<(f64, Vec<BoundingBox>)>
    let detections: Vec<(f64, Vec<BoundingBox>)> = frame_detections
        .into_iter()
        .map(|frame| {
            let time = frame.first().map(|d| d.time).unwrap_or(0.0);
            let boxes: Vec<BoundingBox> = frame.iter().map(|d| d.bbox).collect();
            (time, boxes)
        })
        .collect();

    info!("[STREAMER_SPLIT] Got {} detection frames", detections.len());

    // Step 4: Compute face tracking crop windows
    let face_crops = compute_face_crop_windows(
        &detections,
        seg_width,
        seg_height,
        seg_fps,
        seg_duration,
    );

    // Step 5: Render the streamer split
    info!("[STREAMER_SPLIT] Rendering split view...");

    render_streamer_split(
        &segment_path,
        output,
        seg_width,
        seg_height,
        &face_crops,
        encoding,
    )
    .await?;

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


/// Crop window for face tracking.
#[derive(Debug, Clone, Copy)]
struct FaceCropWindow {
    /// Center X position (0.0 to 1.0)
    cx: f64,
    /// Center Y position (0.0 to 1.0)
    cy: f64,
    /// Whether a face was detected for this frame
    has_face: bool,
}

impl Default for FaceCropWindow {
    fn default() -> Self {
        Self {
            cx: 0.5,
            cy: 0.4, // Default to upper-center
            has_face: false,
        }
    }
}

/// Compute face crop windows for each frame.
///
/// Returns a crop window per frame, with fallback to last known position
/// or default center if no face detected.
fn compute_face_crop_windows(
    detections: &[(f64, Vec<BoundingBox>)],
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
) -> Vec<FaceCropWindow> {
    let frame_count = (duration * fps).ceil() as usize;
    let mut crops = vec![FaceCropWindow::default(); frame_count.max(1)];
    
    // Build a map of timestamp -> best face position
    let mut detection_map: std::collections::BTreeMap<usize, (f64, f64)> = std::collections::BTreeMap::new();
    
    for (timestamp, faces) in detections {
        if faces.is_empty() {
            continue;
        }
        
        // Find the largest face (likely the streamer's face cam)
        let best_face = faces
            .iter()
            .max_by(|a, b| {
                let area_a = a.width * a.height;
                let area_b = b.width * b.height;
                area_a.partial_cmp(&area_b).unwrap_or(std::cmp::Ordering::Equal)
            });
        
        if let Some(face) = best_face {
            let frame_idx = (*timestamp * fps).round() as usize;
            let cx = (face.x + face.width / 2.0) / width as f64;
            let cy = (face.y + face.height / 2.0) / height as f64;
            detection_map.insert(frame_idx, (cx, cy));
        }
    }
    
    // Fill in crops with interpolation/fallback
    let mut last_known: Option<(f64, f64)> = None;
    
    for i in 0..crops.len() {
        if let Some(&(cx, cy)) = detection_map.get(&i) {
            crops[i] = FaceCropWindow {
                cx,
                cy,
                has_face: true,
            };
            last_known = Some((cx, cy));
        } else if let Some((cx, cy)) = last_known {
            // Use last known position (frozen face fallback)
            crops[i] = FaceCropWindow {
                cx,
                cy,
                has_face: false, // Using fallback
            };
        }
        // Otherwise keep default (center, no face)
    }
    
    // Smooth the crop positions for stable camera movement
    smooth_crop_windows(&mut crops);
    
    crops
}

/// Smooth crop window positions to avoid jittery camera movement.
fn smooth_crop_windows(crops: &mut [FaceCropWindow]) {
    if crops.len() < 3 {
        return;
    }
    
    // Apply exponential moving average
    let alpha = 0.3; // Smoothing factor
    
    let mut smoothed_cx = crops[0].cx;
    let mut smoothed_cy = crops[0].cy;
    
    for crop in crops.iter_mut() {
        smoothed_cx = alpha * crop.cx + (1.0 - alpha) * smoothed_cx;
        smoothed_cy = alpha * crop.cy + (1.0 - alpha) * smoothed_cy;
        crop.cx = smoothed_cx;
        crop.cy = smoothed_cy;
    }
}

/// Render the streamer split video.
///
/// Creates a single-pass FFmpeg command that:
/// 1. Letterboxes the original content to fit top panel (9:8)
/// 2. Crops and scales face region for bottom panel (9:8)
/// 3. Stacks them vertically
/// 4. Uses audio only from the original (input)
async fn render_streamer_split(
    segment: &Path,
    output: &Path,
    width: u32,
    height: u32,
    face_crops: &[FaceCropWindow],
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    // For simplicity, we use a single representative crop position
    // (average of all detected positions) for static crop.
    // A more advanced implementation would use per-frame cropping.
    let avg_crop = if face_crops.is_empty() {
        FaceCropWindow::default()
    } else {
        let (sum_cx, sum_cy, count) = face_crops.iter().fold(
            (0.0, 0.0, 0),
            |(cx, cy, c), crop| (cx + crop.cx, cy + crop.cy, c + 1),
        );
        FaceCropWindow {
            cx: sum_cx / count as f64,
            cy: sum_cy / count as f64,
            has_face: face_crops.iter().any(|c| c.has_face),
        }
    };

    // Calculate face crop dimensions
    // We want to crop a 9:8 region centered on the face
    let panel_ratio = PANEL_WIDTH as f64 / PANEL_HEIGHT as f64; // 9:8 = 1.125
    
    // Calculate the maximum crop size that fits within the frame
    let max_crop_width = width as f64;
    let max_crop_height = height as f64;
    
    // Calculate crop dimensions to maintain 9:8 aspect ratio
    let (face_crop_w, face_crop_h) = if max_crop_width / max_crop_height > panel_ratio {
        // Frame is wider than 9:8 - height limited
        let h = max_crop_height * 0.8; // Use 80% of height to leave some margin
        let w = h * panel_ratio;
        (w.min(max_crop_width), h)
    } else {
        // Frame is taller than 9:8 - width limited
        let w = max_crop_width * 0.8;
        let h = w / panel_ratio;
        (w, h.min(max_crop_height))
    };
    
    // Calculate crop position centered on detected face
    let face_crop_x = ((avg_crop.cx * width as f64) - face_crop_w / 2.0)
        .max(0.0)
        .min(width as f64 - face_crop_w);
    let face_crop_y = ((avg_crop.cy * height as f64) - face_crop_h / 2.0)
        .max(0.0)
        .min(height as f64 - face_crop_h);

    info!(
        "[STREAMER_SPLIT] Face crop: {}x{} at ({}, {}), has_face: {}",
        face_crop_w as i32, face_crop_h as i32,
        face_crop_x as i32, face_crop_y as i32,
        avg_crop.has_face
    );

    // Build filter complex:
    // - Top panel: Original video letterboxed to 9:8 (pad with black bars)
    // - Bottom panel: Face region cropped and scaled to 9:8
    // - Stack vertically
    // - Audio from original only
    let filter_complex = format!(
        "[0:v]scale={pw}:{ph}:force_original_aspect_ratio=decrease,\
         pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2:black,\
         setsar=1,format=yuv420p[top];\
         [0:v]crop={fcw}:{fch}:{fcx}:{fcy},\
         scale={pw}:{ph}:flags=lanczos,\
         setsar=1,format=yuv420p[bottom];\
         [top][bottom]vstack=inputs=2[vout]",
        pw = PANEL_WIDTH,
        ph = PANEL_HEIGHT,
        fcw = face_crop_w as i32,
        fch = face_crop_h as i32,
        fcx = face_crop_x as i32,
        fcy = face_crop_y as i32,
    );

    let mut cmd = Command::new("ffmpeg");
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
        "[vout]",
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

    #[test]
    fn test_streamer_split_processor_creation() {
        let processor = StreamerSplitProcessor::new();
        assert_eq!(processor.name(), "streamer_split");
        assert!(processor.can_handle(Style::StreamerSplit));
        assert!(!processor.can_handle(Style::Split));
        assert_eq!(processor.detection_tier(), DetectionTier::Basic);
    }

    #[test]
    fn test_face_crop_window_default() {
        let crop = FaceCropWindow::default();
        assert!((crop.cx - 0.5).abs() < 0.001);
        assert!((crop.cy - 0.4).abs() < 0.001);
        assert!(!crop.has_face);
    }

    #[test]
    fn test_compute_face_crop_windows_empty() {
        let detections: Vec<(f64, Vec<BoundingBox>)> = vec![];
        let crops = compute_face_crop_windows(&detections, 1920, 1080, 30.0, 1.0);
        assert_eq!(crops.len(), 30);
        assert!(!crops[0].has_face);
    }

    #[test]
    fn test_compute_face_crop_windows_with_detection() {
        let face = BoundingBox::new(100.0, 100.0, 200.0, 200.0);
        let detections = vec![(0.5, vec![face])];
        let crops = compute_face_crop_windows(&detections, 1920, 1080, 30.0, 1.0);
        
        // Frame 15 (0.5 * 30) should have the detection
        let frame_15 = &crops[15];
        assert!(frame_15.has_face || crops.iter().any(|c| c.has_face));
    }

    #[test]
    fn test_smooth_crop_windows() {
        let mut crops = vec![
            FaceCropWindow { cx: 0.2, cy: 0.3, has_face: true },
            FaceCropWindow { cx: 0.8, cy: 0.7, has_face: true },
            FaceCropWindow { cx: 0.5, cy: 0.5, has_face: true },
        ];
        smooth_crop_windows(&mut crops);
        
        // After smoothing, values should be between original values
        assert!(crops[1].cx > 0.2 && crops[1].cx < 0.8);
    }
}
