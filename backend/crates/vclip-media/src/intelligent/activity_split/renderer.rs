//! Rendering for Smart Split (Activity).
//!
//! Renders planned layout spans by generating panel crops and stacking them into
//! a 9:16 portrait output. All spans are scaled to 1080x1920 to keep outputs
//! consistent for concatenation.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use tokio::process::Command;

use super::layout_planner::{LayoutMode, LayoutSpan};
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::crop_planner::CropPlanner;
use crate::intelligent::models::{AspectRatio, CropWindow, Detection, FrameDetections};
use crate::intelligent::single_pass_renderer::SinglePassRenderer;
use crate::watermark::WatermarkConfig;
use crate::intelligent::smoother::CameraSmoother;
use tracing::info;
use vclip_models::EncodingConfig;

pub(crate) struct ActivitySplitRenderer {
    config: IntelligentCropConfig,
    encoding: EncodingConfig,
    frame_width: u32,
    frame_height: u32,
    sample_interval: f64,
    watermark: Option<WatermarkConfig>,
}

impl ActivitySplitRenderer {
    pub fn new(
        config: IntelligentCropConfig,
        encoding: EncodingConfig,
        frame_width: u32,
        frame_height: u32,
        sample_interval: f64,
        watermark: Option<WatermarkConfig>,
    ) -> Self {
        Self {
            config,
            encoding,
            frame_width,
            frame_height,
            sample_interval,
            watermark,
        }
    }

    pub async fn render(
        &self,
        segment: &Path,
        output: &Path,
        detections: &[FrameDetections],
        spans: &[LayoutSpan],
    ) -> MediaResult<()> {
        if spans.is_empty() {
            return Err(MediaError::InvalidVideo(
                "Smart Split (Activity) could not render because no layout spans were provided"
                    .to_string(),
            ));
        }

        let temp_dir = TempDir::new()?;
        let mut span_outputs: Vec<PathBuf> = Vec::new();

        for (idx, span) in spans.iter().enumerate() {
            let target_path = temp_dir.path().join(format!("span_{idx}.mp4"));
            
            // Log span being rendered
            info!(
                span_idx = idx,
                start = format!("{:.2}s", span.start),
                end = format!("{:.2}s", span.end),
                duration = format!("{:.2}s", span.end - span.start),
                layout = ?span.layout,
                "Rendering span"
            );
            
            match span.layout {
                LayoutMode::Full { primary } => {
                    info!(primary = primary, "Rendering Full layout span with SINGLE-PASS");
                    let windows = self.track_windows(detections, primary, span, AspectRatio::PORTRAIT)?;
                    
                    // Use SinglePassRenderer to render directly to target size (1080x1920)
                    // This combines crop, scale and PAD/SAR in one pass
                    
                    // Extract span source first (stream copy)
                    let span_duration = span.end - span.start;
                    let span_segment = temp_dir.path().join(format!("span_segment_full_{idx}.mp4"));
                    
                    self.extract_span_source(segment, &span_segment, span.start, span_duration).await?;
                    
                    let mut renderer = SinglePassRenderer::new(self.config.clone());
                    if let Some(config) = self.watermark.as_ref() {
                        renderer = renderer.with_watermark(config.clone());
                    }
                    renderer
                        .render_full(&span_segment, &target_path, &windows, &self.encoding)
                        .await?;
                }
                LayoutMode::Split { primary, secondary } => {
                    info!(primary = primary, secondary = secondary, "Rendering Split layout span with SINGLE-PASS");
                    
                    // For split spans, we need to render a split view for this time range
                    // We'll use a simplified approach: extract this span's time range, then use SinglePassRenderer
                    let span_duration = span.end - span.start;
                    let span_segment = temp_dir.path().join(format!("span_segment_{idx}.mp4"));
                    
                    self.extract_span_source(segment, &span_segment, span.start, span_duration).await?;
                    
                    // Use SinglePassRenderer for split (SINGLE ENCODE instead of 3)
                    let mut renderer = SinglePassRenderer::new(self.config.clone());
                    if let Some(config) = self.watermark.as_ref() {
                        renderer = renderer.with_watermark(config.clone());
                    }
                    renderer.render_split(
                        &span_segment,
                        &target_path,
                        self.frame_width,
                        self.frame_height,
                        0.0,  // left vertical bias
                        0.15, // right vertical bias
                        0.5,  // left horizontal center (default, no face detection)
                        0.5,  // right horizontal center (default, no face detection)
                        &self.encoding,
                    ).await?;
                }
            }
            span_outputs.push(target_path);
        }

        if span_outputs.len() == 1 {
            crate::fs_utils::move_file(&span_outputs[0], output).await?;
            return Ok(());
        }

        self.concat_segments(&span_outputs, output).await
    }

    fn track_windows(
        &self,
        detections: &[FrameDetections],
        track_id: u32,
        span: &LayoutSpan,
        aspect: AspectRatio,
    ) -> MediaResult<Vec<CropWindow>> {
        let frames = self.frames_for_span(detections, track_id, span)?;
        if frames.is_empty() {
            return Err(MediaError::detection_failed(
                "No face track available for requested layout span",
            ));
        }

        let smoother = CameraSmoother::new(self.config.clone(), 1.0 / self.sample_interval);
        let keyframes =
            smoother.compute_camera_plan(&frames, self.frame_width, self.frame_height, span.start, span.end);
        if keyframes.is_empty() {
            return Err(MediaError::detection_failed(
                "Unable to compute camera plan for Smart Split (Activity)",
            ));
        }

        let planner = CropPlanner::new(self.config.clone(), self.frame_width, self.frame_height);
        Ok(planner.compute_crop_windows(&keyframes, &aspect))
    }

    fn frames_for_span(
        &self,
        detections: &[FrameDetections],
        track_id: u32,
        span: &LayoutSpan,
    ) -> MediaResult<Vec<FrameDetections>> {
        let mut frames: Vec<(f64, Option<Detection>)> = Vec::new();
        let start_idx = (span.start / self.sample_interval).floor() as usize;
        let end_idx = ((span.end / self.sample_interval).ceil() as usize).min(detections.len());

        if start_idx >= detections.len() {
            return Err(MediaError::detection_failed(
                "Span start exceeds available detection data",
            ));
        }

        let mut last_det: Option<Detection> = None;
        let mut first_det: Option<Detection> = None;
        let mut first_det_offset: Option<usize> = None;
        for idx in start_idx..end_idx {
            let frame_time = span.start + (idx - start_idx) as f64 * self.sample_interval;
            let frame = detections.get(idx).cloned().unwrap_or_default();
            if let Some(det) = frame.iter().find(|d| d.track_id == track_id) {
                let mut det = det.clone();
                det.time = frame_time;
                if first_det.is_none() {
                    first_det = Some(det.clone());
                    first_det_offset = Some(idx - start_idx);
                }
                last_det = Some(det.clone());
                frames.push((frame_time, Some(det)));
            } else {
                frames.push((frame_time, last_det.clone()));
            }
        }

        let first_det = first_det.ok_or_else(|| {
            MediaError::detection_failed("Missing face data for Smart Split (Activity)")
        })?;

        // Backfill leading frames before the first detection
        if let Some(first_idx) = first_det_offset {
            for idx in 0..first_idx {
                let frame_time = frames[idx].0;
                let mut det = first_det.clone();
                det.time = frame_time;
                frames[idx].1 = Some(det);
            }
        }

        // Ensure all frames have a detection (carry forward last seen)
        let mut carry = None;
        for (time, det_opt) in frames.iter_mut() {
            if let Some(det) = det_opt.clone() {
                carry = Some(det);
            } else if let Some(mut det) = carry.clone() {
                det.time = *time;
                *det_opt = Some(det);
            }
        }

        let filled_frames: Vec<FrameDetections> = frames
            .into_iter()
            .filter_map(|(time, det_opt)| det_opt.map(|mut det| {
                det.time = time;
                vec![det]
            }))
            .collect();

        if filled_frames.is_empty() {
            return Err(MediaError::detection_failed(
                "Unable to compute camera plan for Smart Split (Activity)",
            ));
        }

        Ok(filled_frames)
    }

    async fn extract_span_source(
        &self,
        full_source: &Path,
        output: &Path,
        start: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let mut extract_cmd = Command::new("ffmpeg");
        extract_cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel", "error",
            "-ss", &format!("{:.3}", start),
            "-i", full_source.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-c", "copy",
            output.to_str().unwrap_or(""),
        ]);
        
        let extract_result = extract_cmd.output().await?;
        if !extract_result.status.success() {
            return Err(MediaError::ffmpeg_failed(
                "Failed to extract span segment",
                Some(String::from_utf8_lossy(&extract_result.stderr).to_string()),
                extract_result.status.code(),
            ));
        }
        Ok(())
    }

    /// Concatenate segments using stream copy.
    ///
    /// Since `scale_to_portrait` already applies `setpts=PTS-STARTPTS` to normalize
    /// timestamps, we can safely use `-c:v copy` here to avoid re-encoding and
    /// keep file sizes small.
    async fn concat_segments(&self, inputs: &[PathBuf], output: &Path) -> MediaResult<()> {
        let list_path = output.with_extension("concat.txt");
        let mut list_body = String::new();
        for path in inputs {
            list_body.push_str("file '");
            list_body.push_str(path.to_string_lossy().as_ref());
            list_body.push_str("'\n");
        }
        tokio::fs::write(&list_path, &list_body)
            .await
            .map_err(MediaError::from)?;

        // Use stream copy - segments are already timestamp-normalized by scale_to_portrait
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", list_path.to_str().unwrap_or_default(),
                // Video: stream copy (no re-encode)
                "-c:v", "copy",
                // Audio: copy or light re-encode for sync
                "-c:a", "aac",
                "-b:a", &self.encoding.audio_bitrate,
                "-movflags", "+faststart",
                output.to_str().unwrap_or_default(),
            ])
            .output()
            .await
            .map_err(|e| MediaError::ffmpeg_failed(format!("Concat failed: {}", e), None, None))?;

        if !status.status.success() {
            return Err(MediaError::ffmpeg_failed(
                "Concat failed",
                Some(String::from_utf8_lossy(&status.stderr).to_string()),
                status.status.code(),
            ));
        }

        // Clean up concat list
        let _ = tokio::fs::remove_file(list_path).await;

        Ok(())
    }
}
