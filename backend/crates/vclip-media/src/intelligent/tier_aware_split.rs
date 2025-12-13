//! Tier-aware split video processing.
//!
//! This module extends the split view processing with tier-specific behavior:
//! - **Basic**: Fixed vertical positioning (current behavior)
//! - **SpeakerAware**: Dynamic per-panel positioning based on face detection
//!
//! For split view styles, the tier primarily affects:
//! 1. Per-panel vertical positioning based on detected face positions
//! 2. Logging and metrics for tier-specific processing

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::detector::FaceDetector;
use super::motion::MotionDetector;
use super::models::BoundingBox;
use super::output_format::{SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT};
use super::single_pass_renderer::SinglePassRenderer;
use crate::clip::extract_segment;
use crate::detection::pipeline_builder::PipelineBuilder;
use crate::error::MediaResult;
use crate::intelligent::Detection;
use crate::intelligent::split_evaluator::SplitEvaluator;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Tier-aware split processor.
pub struct TierAwareSplitProcessor {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    detector: FaceDetector,
}

impl TierAwareSplitProcessor {
    /// Create a new tier-aware split processor.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self {
            detector: FaceDetector::new(config.clone()),
            config,
            tier,
        }
    }

    /// Create with default configuration.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self::new(IntelligentCropConfig::default(), tier)
    }

    /// Get the detection tier.
    pub fn tier(&self) -> DetectionTier {
        self.tier
    }

    /// Process a video segment with tier-aware split using SINGLE-PASS encoding.
    ///
    /// This uses SinglePassRenderer to apply all transforms (crop, scale, vstack)
    /// in ONE FFmpeg command, avoiding multiple encode passes.
    ///
    /// # Smart Split Detection
    /// Before splitting, we check if faces appear SIMULTANEOUSLY for at least 3 seconds.
    /// If not (e.g., camera switches between showing one person then another),
    /// we fallback to full-frame intelligent cropping which handles alternating
    /// speakers much better.
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        self.process_with_cached_detections(segment, output, encoding, None).await
    }

    /// Process a video segment with optional cached neural analysis.
    ///
    /// This is the cache-aware entry point that allows skipping expensive ML inference
    /// when cached detections are available.
    pub async fn process_with_cached_detections<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
        cached_analysis: Option<&vclip_models::SceneNeuralAnalysis>,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();
        let pipeline_start = std::time::Instant::now();

        info!("[INTELLIGENT_SPLIT] ========================================");
        info!("[INTELLIGENT_SPLIT] START: {:?}", segment);
        info!("[INTELLIGENT_SPLIT] Tier: {:?}", self.tier);
        info!("[INTELLIGENT_SPLIT] Cached analysis: {}", cached_analysis.is_some());

        // Step 1: Get video metadata
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 1/4: Probing video metadata...");
        
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "[INTELLIGENT_SPLIT] Step 1/4 DONE in {:.2}s - {}x{} @ {:.2}fps, {:.2}s",
            step_start.elapsed().as_secs_f64(),
            width, height, fps, duration
        );

        // Step 2: Check if split mode is appropriate (simultaneous faces detection)
        // Use cached analysis if available to avoid redundant detection
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 2/4: Checking for simultaneous face presence...");
        
        let should_split = if let Some(analysis) = cached_analysis {
            // Use cached analysis to determine split layout
            self.should_use_split_layout_from_cache(analysis, width, height, duration)
        } else {
            self.should_use_split_layout(segment, width, height, fps, duration).await
        };
        
        info!(
            "[INTELLIGENT_SPLIT] Step 2/4 DONE in {:.2}s - should_split: {}",
            step_start.elapsed().as_secs_f64(),
            should_split
        );
        
        if !should_split {
            info!("[INTELLIGENT_SPLIT] Alternating/single-face detected → using full-frame tracking");
            let cropper = super::tier_aware_cropper::TierAwareIntelligentCropper::new(
                self.config.clone(),
                self.tier,
            );
            cropper.process_with_cached_detections(segment, output, encoding, cached_analysis).await?;
            
            // Generate thumbnail
            let thumb_path = output.with_extension("jpg");
            if let Err(e) = generate_thumbnail(output, &thumb_path).await {
                tracing::warn!("[INTELLIGENT_SPLIT] Failed to generate thumbnail: {}", e);
            }
            
            info!("[INTELLIGENT_SPLIT] ========================================");
            info!(
                "[INTELLIGENT_SPLIT] COMPLETE (full-frame) in {:.2}s",
                pipeline_start.elapsed().as_secs_f64()
            );
            return Ok(());
        }

        // Speaker-aware split uses dedicated mouth-openness path.
        if self.tier == DetectionTier::SpeakerAware {
            info!("[INTELLIGENT_SPLIT] Using SpeakerAware processing path");
            if let Err(e) =
                self.process_speaker_aware_split(segment, output, width, height, duration, encoding).await
            {
                tracing::warn!("[INTELLIGENT_SPLIT] SpeakerAware failed, falling back: {}", e);
            } else {
                return Ok(());
            }
        }

        // Step 2: Compute vertical positioning per tier
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 2/3: Computing vertical positioning...");
        
        let (left_vertical_bias, right_vertical_bias) = match self.tier {
            DetectionTier::MotionAware => {
                info!("[INTELLIGENT_SPLIT]   Using motion-aware positioning");
                self.compute_motion_positioning(segment, width, height, duration)?
            }
            tier if tier.requires_yunet() => {
                info!("[INTELLIGENT_SPLIT]   Using face-aware positioning");
                self.compute_face_aware_positioning(segment, width, height, duration).await
            }
            _ => {
                info!("[INTELLIGENT_SPLIT]   Using fixed positioning (Basic tier)");
                (0.0, 0.15)
            }
        };

        info!(
            "[INTELLIGENT_SPLIT] Step 2/3 DONE in {:.2}s - left={:.2}, right={:.2}",
            step_start.elapsed().as_secs_f64(),
            left_vertical_bias, right_vertical_bias
        );

        // Step 3: Single-pass render (THE ONLY ENCODE)
        info!("[INTELLIGENT_SPLIT] Step 3/3: Single-pass encoding...");
        info!("[INTELLIGENT_SPLIT]   Encoding: {} preset={} crf={}", 
            encoding.codec, encoding.preset, encoding.crf);
        
        let renderer = SinglePassRenderer::new(self.config.clone());
        renderer.render_split(
            segment,
            output,
            width,
            height,
            left_vertical_bias,
            right_vertical_bias,
            encoding,
        )
        .await?;

        // Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("[INTELLIGENT_SPLIT] Failed to generate thumbnail: {}", e);
        }

        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!("[INTELLIGENT_SPLIT] ========================================");
        info!(
            "[INTELLIGENT_SPLIT] COMPLETE in {:.2}s - {:.2} MB",
            pipeline_start.elapsed().as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
    }

    /// Compute motion-aware vertical positioning for each panel (NN-free).
    ///
    /// Uses dual MotionDetector instances on left/right halves and returns
    /// (top_bias, bottom_bias) where 0.0 = top, 1.0 = bottom.
    fn compute_motion_positioning<P: AsRef<Path>>(
        &self,
        segment: P,
        width: u32,
        height: u32,
        duration: f64,
    ) -> MediaResult<(f64, f64)> {
        use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
        use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_MSEC};

        let segment = segment.as_ref();
        let half_width = (width / 2) as i32;
        let height_i = height as i32;

        let mut cap = VideoCapture::from_file(segment.to_str().unwrap_or(""), CAP_ANY)
            .map_err(|e| crate::error::MediaError::detection_failed(format!("Open video: {e}")))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(crate::error::MediaError::detection_failed(
                "Failed to open video for motion analysis",
            ));
        }

        let mut left_motion = MotionDetector::new(half_width, height_i);
        let mut right_motion = MotionDetector::new(half_width, height_i);

        let sample_interval = 1.0 / self.config.fps_sample.max(1e-3);
        let mut current_time = 0.0;
        let mut left_biases = Vec::new();
        let mut right_biases = Vec::new();

        // Coasting: hold last valid motion target briefly to reduce jitter.
        const DECAY_SECONDS: f64 = 2.0;
        let mut last_left: Option<(f64, f64)> = None; // (time, bias)
        let mut last_right: Option<(f64, f64)> = None;

        while current_time < duration {
            cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Seek: {e}")))?;

            let mut frame = opencv::core::Mat::default();
            if !cap
                .read(&mut frame)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))? || frame.empty()
            {
                current_time += sample_interval;
                continue;
            }

            // Split frame into left/right halves
            let left_roi = opencv::core::Rect::new(0, 0, half_width, height_i);
            let right_roi = opencv::core::Rect::new(half_width, 0, half_width, height_i);

            let mut left_bias_opt = None;
            if let Ok(roi) = opencv::core::Mat::roi(&frame, left_roi) {
                let mut roi_mat = opencv::core::Mat::default();
                if roi.copy_to(&mut roi_mat).is_ok() {
                    if let Ok(center_opt) = left_motion.detect_center(&roi_mat) {
                        if let Some(center) = center_opt {
                            left_bias_opt = Some((center.y as f64 / height as f64).clamp(0.0, 1.0));
                        }
                    }
                }
            }

            let mut right_bias_opt = None;
            if let Ok(roi) = opencv::core::Mat::roi(&frame, right_roi) {
                let mut roi_mat = opencv::core::Mat::default();
                if roi.copy_to(&mut roi_mat).is_ok() {
                    if let Ok(center_opt) = right_motion.detect_center(&roi_mat) {
                        if let Some(center) = center_opt {
                            right_bias_opt = Some((center.y as f64 / height as f64).clamp(0.0, 1.0));
                        }
                    }
                }
            }

            // Coasting logic
            let now = current_time;
            if let Some(b) = left_bias_opt {
                last_left = Some((now, b));
                left_biases.push(b);
            } else if let Some((t, b)) = last_left {
                if now - t <= DECAY_SECONDS {
                    left_biases.push(b);
                }
            }

            if let Some(b) = right_bias_opt {
                last_right = Some((now, b));
                right_biases.push(b);
            } else if let Some((t, b)) = last_right {
                if now - t <= DECAY_SECONDS {
                    right_biases.push(b);
                }
            }

            current_time += sample_interval;
        }

        let avg = |vals: &[f64]| -> f64 {
            if vals.is_empty() {
                0.15
            } else {
                (vals.iter().sum::<f64>() / vals.len() as f64).clamp(0.0, 1.0)
            }
        };

        Ok((avg(&left_biases), avg(&right_biases)))
    }

    /// Compute face-aware vertical positioning for each panel.
    ///
    /// Returns (left_bias, right_bias) where 0.0 = top, 1.0 = bottom.
    async fn compute_face_aware_positioning<P: AsRef<Path>>(
        &self,
        segment: P,
        width: u32,
        height: u32,
        duration: f64,
    ) -> (f64, f64) {
        let segment = segment.as_ref();

        // Sample a few frames to detect face positions
        let sample_duration = duration.min(5.0); // Sample first 5 seconds
        let fps = self.config.fps_sample;

        match self.detector.detect_in_video(
            segment,
            0.0,
            sample_duration,
            width,
            height,
            fps,
        ).await {
            Ok(detections) => {
                // Analyze face positions in left and right halves
                let center_x = width as f64 / 2.0;
                let mut left_faces: Vec<&BoundingBox> = Vec::new();
                let mut right_faces: Vec<&BoundingBox> = Vec::new();

                for frame_dets in &detections {
                    for det in frame_dets {
                        if det.bbox.cx() < center_x {
                            left_faces.push(&det.bbox);
                        } else {
                            right_faces.push(&det.bbox);
                        }
                    }
                }

                // Compute average vertical position for each side
                let left_bias = self.compute_vertical_bias(&left_faces, height);
                let right_bias = self.compute_vertical_bias(&right_faces, height);

                info!(
                    "Face detection: {} left faces, {} right faces",
                    left_faces.len(),
                    right_faces.len()
                );

                (left_bias, right_bias)
            }
            Err(e) => {
                tracing::warn!("Face detection failed, using defaults: {}", e);
                (0.0, 0.15)
            }
        }
    }

    /// Speaker-aware split path with mouth-open activity and robust left/right mapping.
    async fn process_speaker_aware_split(
        &self,
        segment: &Path,
        output: &Path,
        width: u32,
        height: u32,
        duration: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        let center_x = width as f64 / 2.0;
        let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware).build()?;
        let result = pipeline.analyze(segment, 0.0, duration).await?;
        if result.frames.is_empty() {
            return Err(crate::error::MediaError::detection_failed(
                "Speaker-aware pipeline returned no frames",
            ));
        }
        let frames: Vec<Vec<Detection>> = result.frames.iter().map(|f| f.faces.clone()).collect();

        let split_eval = SplitEvaluator::evaluate_speaker_split(&frames, width, height, duration);

        if split_eval.is_none() {
            tracing::info!(
                "Speaker-aware split: not enough dual activity -> single view"
            );
            let cropper = super::tier_aware_cropper::TierAwareIntelligentCropper::new(
                self.config.clone(),
                DetectionTier::SpeakerAware,
            );
            return cropper.process(segment, output, encoding).await;
        }

        let (left_box, right_box) = split_eval.unwrap();

        // Width tuned to keep single speaker per panel
        let crop_width_left = left_box.width.min(width as f64 * 0.55).max(width as f64 * 0.25);
        let crop_width_right = right_box.width.min(width as f64 * 0.55).max(width as f64 * 0.25);

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

        let left_bias = (left_box.cy() / height as f64 - 0.3).max(0.0).min(0.4);
        let right_bias = (right_box.cy() / height as f64 - 0.3).max(0.0).min(0.4);

        let left_crop_y = (vertical_margin_left * left_bias).round();
        let right_crop_y = (vertical_margin_right * right_bias).round();

        // Clamp crop coordinates to ensure validity
        use super::output_format::clamp_crop_to_frame;
        let (left_crop_x, left_crop_y, crop_width_left_u32, tile_height_left_u32) = clamp_crop_to_frame(
            left_crop_x as i32,
            left_crop_y as i32,
            crop_width_left as i32,
            tile_height_left as i32,
            width,
            height,
        );
        let (right_crop_x, right_crop_y, crop_width_right_u32, tile_height_right_u32) = clamp_crop_to_frame(
            right_crop_x as i32,
            right_crop_y as i32,
            crop_width_right as i32,
            tile_height_right as i32,
            width,
            height,
        );

        info!("[SPEAKER_SPLIT] Using SINGLE-PASS encoding with custom speaker crops...");
        info!("[SPEAKER_SPLIT]   Left: crop {}x{} at ({}, {})", 
            crop_width_left_u32, tile_height_left_u32, left_crop_x, left_crop_y);
        info!("[SPEAKER_SPLIT]   Right: crop {}x{} at ({}, {})",
            crop_width_right_u32, tile_height_right_u32, right_crop_x, right_crop_y);

        // Build combined filter graph for SINGLE-PASS encoding
        // This is more complex than the standard split because each side has different crop dimensions
        // Uses centralized SPLIT_PANEL dimensions for consistent 9:16 output
        let filter_complex = format!(
            "[0:v]split=2[left_in][right_in];\
             [left_in]crop={lw}:{lth}:{lx}:{ly},scale={pw}:{ph}:flags=lanczos:force_original_aspect_ratio=decrease,pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p[top];\
             [right_in]crop={rw}:{rth}:{rx}:{ry},scale={pw}:{ph}:flags=lanczos:force_original_aspect_ratio=decrease,pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p[bottom];\
             [top][bottom]vstack=inputs=2[vout]",
            lw = crop_width_left_u32,
            lx = left_crop_x,
            lth = tile_height_left_u32,
            ly = left_crop_y,
            rw = crop_width_right_u32,
            rx = right_crop_x,
            rth = tile_height_right_u32,
            ry = right_crop_y,
            pw = SPLIT_PANEL_WIDTH,
            ph = SPLIT_PANEL_HEIGHT,
        );

        use std::process::Stdio;
        use tokio::process::Command;

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel", "error",
            "-i", segment.to_str().unwrap_or(""),
            "-filter_complex", &filter_complex,
            "-map", "[vout]",
            "-map", "0:a?",
            // SINGLE ENCODE using API config
            "-c:v", &encoding.codec,
            "-preset", &encoding.preset,
            "-crf", &encoding.crf.to_string(),
            "-pix_fmt", "yuv420p",
            "-c:a", "aac",
            "-b:a", &encoding.audio_bitrate,
            "-movflags", "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let result = cmd.output().await.map_err(|e| {
            crate::error::MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(crate::error::MediaError::ffmpeg_failed(
                "Speaker-aware split render failed",
                Some(stderr.to_string()),
                result.status.code(),
            ));
        }

        info!("[SPEAKER_SPLIT] Single-pass encoding complete");
        Ok(())
    }

    /// Compute vertical bias from detected faces.
    ///
    /// Returns a value from 0.0 (top) to 1.0 (bottom) indicating where
    /// to position the crop to best capture the faces.
    fn compute_vertical_bias(&self, faces: &[&BoundingBox], height: u32) -> f64 {
        if faces.is_empty() {
            return 0.15; // Default: slight bias toward top
        }

        // Compute average face center Y position
        let avg_cy: f64 = faces.iter().map(|f| f.cy()).sum::<f64>() / faces.len() as f64;

        // Convert to bias (0.0 = face at top, 1.0 = face at bottom)
        let normalized_y = avg_cy / height as f64;

        // We want to position the crop so the face is in the upper portion
        // If face is at 30% of frame height, we want bias ~0.0 (crop from top)
        // If face is at 50% of frame height, we want bias ~0.15
        // If face is at 70% of frame height, we want bias ~0.3

        // Clamp to reasonable range
        let bias = (normalized_y - 0.3).max(0.0).min(0.4);

        bias
    }

    /// Determine if split layout is appropriate using cached analysis.
    ///
    /// Uses pre-computed neural analysis to avoid redundant face detection.
    fn should_use_split_layout_from_cache(
        &self,
        analysis: &vclip_models::SceneNeuralAnalysis,
        _width: u32,
        _height: u32,
        duration: f64,
    ) -> bool {
        const MIN_SIMULTANEOUS_SECONDS: f64 = 3.0;
        
        if analysis.frames.is_empty() {
            info!("[SPLIT_CHECK] No cached frames → using full-frame mode");
            return false;
        }
        
        let sample_interval = duration / analysis.frames.len().max(1) as f64;
        let mut simultaneous_time = 0.0;
        let mut distinct_tracks = std::collections::HashSet::new();
        
        for frame in &analysis.frames {
            if frame.faces.len() >= 2 {
                simultaneous_time += sample_interval;
            }
            for face in &frame.faces {
                if let Some(track_id) = face.track_id {
                    distinct_tracks.insert(track_id);
                }
            }
        }
        
        let should_split = distinct_tracks.len() >= 2 && simultaneous_time >= MIN_SIMULTANEOUS_SECONDS;
        
        info!(
            "[SPLIT_CHECK] (cached) {} tracks, {:.1}s simultaneous (need >= {:.1}s) → {}",
            distinct_tracks.len(),
            simultaneous_time,
            MIN_SIMULTANEOUS_SECONDS,
            if should_split { "SPLIT" } else { "FULL-FRAME" }
        );
        
        should_split
    }

    /// Determine if split layout is appropriate for this video.
    ///
    /// Split layout is only appropriate for TRUE side-by-side podcasts where
    /// both speakers are visible simultaneously. For videos that show one person
    /// at a time (alternating left/right framings), full-frame tracking is better.
    ///
    /// # Algorithm
    /// 1. Sample frames for face detection
    /// 2. Count time where 2+ faces are visible SIMULTANEOUSLY
    /// 3. If simultaneous time >= 3 seconds, use split mode
    /// 4. Otherwise, use full-frame mode
    async fn should_use_split_layout(
        &self,
        segment: &Path,
        width: u32,
        height: u32,
        fps: f64,
        duration: f64,
    ) -> bool {
        // Sample at a reasonable rate (not every frame, that's too expensive)
        let sample_fps = self.config.fps_sample.max(2.0).min(8.0);
        let sample_interval = 1.0 / sample_fps;
        
        // Minimum simultaneous visibility required for split mode
        const MIN_SIMULTANEOUS_SECONDS: f64 = 3.0;
        
        // Detect faces across the video
        let detections = match self.detector.detect_in_video(
            segment,
            0.0,
            duration,
            width,
            height,
            fps,
        ).await {
            Ok(dets) => dets,
            Err(e) => {
                tracing::warn!("[SPLIT_CHECK] Face detection failed: {} → defaulting to split", e);
                return true; // Default to split if detection fails (preserves existing behavior)
            }
        };
        
        if detections.is_empty() {
            info!("[SPLIT_CHECK] No faces detected → using full-frame mode");
            return false;
        }
        
        // Count time with 2+ faces visible simultaneously
        let mut simultaneous_time = 0.0;
        let mut distinct_tracks = std::collections::HashSet::new();
        
        for frame_dets in &detections {
            if frame_dets.len() >= 2 {
                simultaneous_time += sample_interval;
            }
            for det in frame_dets {
                distinct_tracks.insert(det.track_id);
            }
        }
        
        let should_split = distinct_tracks.len() >= 2 && simultaneous_time >= MIN_SIMULTANEOUS_SECONDS;
        
        info!(
            "[SPLIT_CHECK] {} tracks, {:.1}s simultaneous (need >= {:.1}s) → {}",
            distinct_tracks.len(),
            simultaneous_time,
            MIN_SIMULTANEOUS_SECONDS,
            if should_split { "SPLIT" } else { "FULL-FRAME" }
        );
        
        should_split
    }

}

/// Create a tier-aware intelligent split clip from a video file.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Compute vertical positioning per tier
/// 3. `SinglePassRenderer::render_split()` - ONE encode with split filter graph
pub async fn create_tier_aware_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
    progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    // Delegate to cache-aware version with no cache
    create_tier_aware_split_clip_with_cache(
        input,
        output,
        task,
        tier,
        encoding,
        None,
        progress_callback,
    )
    .await
}

/// Create a tier-aware intelligent split clip with optional cached neural analysis.
///
/// This is the cache-aware entry point that allows skipping expensive ML inference
/// when cached detections are available.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Compute vertical positioning per tier (SKIPPED if cache provided)
/// 3. `SinglePassRenderer::render_split()` - ONE encode with split filter graph
pub async fn create_tier_aware_split_clip_with_cache<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
    cached_analysis: Option<&vclip_models::SceneNeuralAnalysis>,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();
    let total_start = std::time::Instant::now();

    info!("========================================================");
    info!("[PIPELINE] INTELLIGENT SPLIT - START");
    info!("[PIPELINE] Source: {:?}", input);
    info!("[PIPELINE] Output: {:?}", output);
    info!("[PIPELINE] Tier: {:?}", tier);
    info!("[PIPELINE] Cached analysis: {}", cached_analysis.is_some());
    info!("[PIPELINE] Encoding: {} crf={}", encoding.codec, encoding.crf);

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    info!("[PIPELINE] Time: {:.2}s to {:.2}s ({:.2}s duration)", start_secs, end_secs, duration);

    // Step 1: Extract segment using STREAM COPY (no encode)
    let segment_path = output.with_extension("segment.mp4");
    info!("[PIPELINE] Step 1/2: Extract segment (STREAM COPY - no encode)...");

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with single-pass render (THE ONLY ENCODE)
    info!("[PIPELINE] Step 2/2: Process segment (SINGLE ENCODE)...");
    
    let config = IntelligentCropConfig::default();
    let processor = TierAwareSplitProcessor::new(config, tier);
    let result = processor
        .process_with_cached_detections(segment_path.as_path(), output, encoding, cached_analysis)
        .await;

    // Cleanup
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!("[PIPELINE] Failed to delete temp segment: {}", e);
        } else {
            info!("[PIPELINE] Cleaned up temp segment");
        }
    }

    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!("========================================================");
    info!(
        "[PIPELINE] INTELLIGENT SPLIT - COMPLETE in {:.2}s - {:.2} MB",
        total_start.elapsed().as_secs_f64(),
        file_size as f64 / 1_000_000.0
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_creation() {
        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::Basic);
        assert_eq!(processor.tier(), DetectionTier::Basic);
    }

    #[test]
    fn test_vertical_bias_computation() {
        let config = IntelligentCropConfig::default();
        let processor = TierAwareSplitProcessor::new(config, DetectionTier::Basic);

        // Face at top of frame -> low bias
        let top_face = BoundingBox::new(100.0, 50.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&top_face], 1080);
        assert!(bias < 0.1, "Top face should have low bias: {}", bias);

        // Face at middle of frame -> medium bias
        let mid_face = BoundingBox::new(100.0, 440.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&mid_face], 1080);
        assert!(bias > 0.1 && bias < 0.3, "Mid face should have medium bias: {}", bias);

        // Face at bottom of frame -> higher bias (clamped)
        let bottom_face = BoundingBox::new(100.0, 800.0, 100.0, 100.0);
        let bias = processor.compute_vertical_bias(&[&bottom_face], 1080);
        assert!(bias >= 0.3, "Bottom face should have higher bias: {}", bias);
    }

    #[test]
    fn test_empty_faces_default_bias() {
        let config = IntelligentCropConfig::default();
        let processor = TierAwareSplitProcessor::new(config, DetectionTier::Basic);

        let bias = processor.compute_vertical_bias(&[], 1080);
        assert!((bias - 0.15).abs() < 0.01, "Empty faces should use default bias");
    }

}
