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
            match span.layout {
                LayoutMode::Full { primary } => {
                    let windows = self.track_windows(detections, primary, span, AspectRatio::PORTRAIT)?;
                    let raw_out = temp_dir.path().join(format!("full_raw_{idx}.mp4"));
                    let renderer = IntelligentRenderer::new(self.config.clone());
                    renderer
                        .render(segment, &raw_out, &windows, span.start, span.end - span.start)
                        .await?;
                    self.scale_to_portrait(&raw_out, &target_path).await?;
                }
                LayoutMode::Split { primary, secondary } => {
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
            tokio::fs::rename(&span_outputs[0], output)
                .await
                .map_err(MediaError::from)?;
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
        let mut frames: Vec<FrameDetections> = Vec::new();
        let start_idx = (span.start / self.sample_interval).floor() as usize;
        let end_idx = ((span.end / self.sample_interval).ceil() as usize).min(detections.len());

        if start_idx >= detections.len() {
            return Err(MediaError::detection_failed(
                "Span start exceeds available detection data",
            ));
        }

        let mut last_det: Option<Detection> = None;
        for idx in start_idx..end_idx {
            let frame_time = span.start + (idx - start_idx) as f64 * self.sample_interval;
            let frame = detections.get(idx).cloned().unwrap_or_default();
            if let Some(det) = frame.iter().find(|d| d.track_id == track_id) {
                let mut det = det.clone();
                det.time = frame_time;
                last_det = Some(det.clone());
                frames.push(vec![det]);
            } else if let Some(prev) = last_det.clone() {
                let mut carry = prev;
                carry.time = frame_time;
                frames.push(vec![carry]);
            } else {
                // Gap at the start of span – treat as fatal
                return Err(MediaError::detection_failed(
                    "Missing face data at span start for Smart Split (Activity)",
                ));
            }
        }

        Ok(frames)
    }

    async fn scale_to_portrait(&self, input: &Path, output: &Path) -> MediaResult<()> {
        let vf = "scale=1080:1920:flags=lanczos";
        let cmd = FfmpegCommand::new(input, output)
            .video_filter(vf)
            .video_codec(&self.encoding.codec)
            .preset(&self.encoding.preset)
            .crf(self.encoding.crf)
            .audio_codec("aac")
            .audio_bitrate(&self.encoding.audio_bitrate);

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
                "-c",
                "copy",
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

