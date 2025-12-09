//! Rendering for Smart Split (Activity).
//!
//! Renders planned layout spans by generating panel crops and stacking them into
//! a 9:16 portrait output. All spans are scaled to 1080x1920 to keep outputs
//! consistent for concatenation.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use tokio::process::Command;

use super::layout_planner::{LayoutMode, LayoutSpan};
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::crop_planner::CropPlanner;
use crate::intelligent::models::{AspectRatio, CropWindow, Detection, FrameDetections};
use crate::intelligent::renderer::IntelligentRenderer;
use crate::intelligent::smoother::CameraSmoother;
use crate::intelligent::stacking::stack_halves;
use tracing::info;
use vclip_models::EncodingConfig;

pub(crate) struct ActivitySplitRenderer {
    config: IntelligentCropConfig,
    encoding: EncodingConfig,
    frame_width: u32,
    frame_height: u32,
    sample_interval: f64,
}

impl ActivitySplitRenderer {
    pub fn new(
        config: IntelligentCropConfig,
        encoding: EncodingConfig,
        frame_width: u32,
        frame_height: u32,
        sample_interval: f64,
    ) -> Self {
        Self {
            config,
            encoding,
            frame_width,
            frame_height,
            sample_interval,
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
                    info!(primary = primary, "Rendering Full layout span");
                    let windows = self.track_windows(detections, primary, span, AspectRatio::PORTRAIT)?;
                    let raw_out = temp_dir.path().join(format!("full_raw_{idx}.mp4"));
                    let renderer = IntelligentRenderer::new(self.config.clone());
                    renderer
                        .render(segment, &raw_out, &windows, span.start, span.end - span.start)
                        .await?;
                    self.scale_to_portrait(&raw_out, &target_path).await?;
                }
                LayoutMode::Split { primary, secondary } => {
                    info!(primary = primary, secondary = secondary, "Rendering Split layout span");
                    let top_windows =
                        self.track_windows(detections, primary, span, AspectRatio::new(9, 8))?;
                    let bottom_windows =
                        self.track_windows(detections, secondary, span, AspectRatio::new(9, 8))?;

                    let top_raw = temp_dir.path().join(format!("top_{idx}.mp4"));
                    let bottom_raw = temp_dir.path().join(format!("bottom_{idx}.mp4"));
                    let renderer = IntelligentRenderer::new(self.config.clone());
                    renderer
                        .render(segment, &top_raw, &top_windows, span.start, span.end - span.start)
                        .await?;
                    renderer
                        .render(
                            segment,
                            &bottom_raw,
                            &bottom_windows,
                            span.start,
                            span.end - span.start,
                        )
                        .await?;

                    let stacked = temp_dir.path().join(format!("stacked_{idx}.mp4"));
                    stack_halves(&top_raw, &bottom_raw, &stacked, &self.encoding).await?;
                    self.scale_to_portrait(&stacked, &target_path).await?;
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

    async fn scale_to_portrait(&self, input: &Path, output: &Path) -> MediaResult<()> {
        let vf = "scale=1080:1920:flags=lanczos";
        let cmd = FfmpegCommand::new(input, output)
            .video_filter(vf)
            .video_codec(&self.encoding.codec)
            .preset(&self.encoding.preset)
            .crf(self.encoding.crf)
            .audio_codec("copy");  // Copy audio to avoid re-encoding artifacts

        FfmpegRunner::new().run(&cmd).await
    }

    async fn concat_segments(&self, inputs: &[PathBuf], output: &Path) -> MediaResult<()> {
        let list_path = output.with_extension("concat.txt");
        let mut list_body = String::new();
        for path in inputs {
            list_body.push_str("file '");
            list_body.push_str(path.to_string_lossy().as_ref());
            list_body.push_str("'\n");
        }
        tokio::fs::write(&list_path, list_body)
            .await
            .map_err(MediaError::from)?;

        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                list_path.to_str().unwrap_or_default(),
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                &self.encoding.audio_bitrate,
                "-af",
                "aresample=async=1:first_pts=0",  // Fix audio discontinuities at segment boundaries
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

