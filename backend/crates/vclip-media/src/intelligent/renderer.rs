//! FFmpeg-based rendering of intelligent cropped videos.
//!
//! Converts crop windows into FFmpeg commands for final rendering.

use super::config::IntelligentCropConfig;
use super::crop_planner::is_static_crop;
use super::models::CropWindow;
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::debug;

/// Renderer for intelligent cropped videos.
pub struct IntelligentRenderer {
    config: IntelligentCropConfig,
}

impl IntelligentRenderer {
    /// Create a new renderer.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self { config }
    }

    /// Render a video with intelligent cropping.
    ///
    /// # Arguments
    /// * `input` - Input video path
    /// * `output` - Output video path
    /// * `crop_windows` - Computed crop windows
    /// * `start_time` - Start time in seconds
    /// * `duration` - Duration in seconds
    pub async fn render<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        start_time: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if crop_windows.is_empty() {
            return Err(MediaError::InvalidVideo("No crop windows to render".to_string()));
        }

        // Determine rendering strategy
        if is_static_crop(crop_windows) {
            debug!("Using static crop rendering");
            self.render_static_crop(input, output, crop_windows, start_time, duration)
                .await
        } else {
            debug!("Using dynamic crop rendering");
            self.render_dynamic_crop(input, output, crop_windows, start_time, duration)
                .await
        }
    }

    /// Render using a single static crop (faster).
    async fn render_static_crop<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        start_time: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        // Use median crop window
        let crop = self.compute_median_crop(crop_windows);

        // Build FFmpeg filter
        let vf = format!(
            "crop={}:{}:{}:{},scale=trunc(iw/2)*2:trunc(ih/2)*2",
            crop.width, crop.height, crop.x, crop.y
        );

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss",
            &format!("{:.3}", start_time),
            "-i",
            input.to_str().unwrap_or(""),
            "-t",
            &format!("{:.3}", duration),
            "-vf",
            &vf,
            "-c:v",
            "libx264",
            "-preset",
            &self.config.render_preset,
            "-crf",
            &self.config.render_crf.to_string(),
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        debug!("Running FFmpeg static crop: {:?}", cmd);

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "FFmpeg failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Render with dynamic cropping (segment-based approach).
    async fn render_dynamic_crop<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        start_time: f64,
        _duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        // Group windows into segments with similar crops
        let segments = self.group_segments(crop_windows);

        if segments.len() == 1 {
            // Single segment - use static crop
            return self
                .render_single_segment(
                    input,
                    output,
                    &segments[0],
                    start_time,
                )
                .await;
        }

        // Multiple segments - render each and concatenate
        self.render_and_concat_segments(input, output, &segments, start_time)
            .await
    }

    /// Group crop windows into segments with similar crops.
    fn group_segments(&self, windows: &[CropWindow]) -> Vec<CropSegment> {
        if windows.is_empty() {
            return Vec::new();
        }

        let tolerance = 0.1; // 10% tolerance
        let mut segments = Vec::new();
        let mut current_start = windows[0].time;
        let mut current_crop = windows[0];

        for window in windows.iter().skip(1) {
            if self.crops_differ(&current_crop, window, tolerance) {
                // End current segment
                segments.push(CropSegment {
                    start: current_start,
                    end: window.time,
                    crop: current_crop,
                });
                current_start = window.time;
                current_crop = *window;
            }
        }

        // Add final segment
        segments.push(CropSegment {
            start: current_start,
            end: windows.last().unwrap().time + 1.0, // Extend slightly
            crop: current_crop,
        });

        segments
    }

    /// Check if two crops are significantly different.
    fn crops_differ(&self, crop1: &CropWindow, crop2: &CropWindow, tolerance: f64) -> bool {
        let threshold = (crop1.width as f64 * tolerance) as i32;
        (crop1.x - crop2.x).abs() > threshold
            || (crop1.y - crop2.y).abs() > threshold
            || (crop1.width - crop2.width).abs() > threshold
    }

    /// Render a single segment.
    async fn render_single_segment<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        segment: &CropSegment,
        base_start: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();
        let crop = &segment.crop;

        let start = base_start + segment.start;
        let duration = segment.end - segment.start;

        let vf = format!(
            "crop={}:{}:{}:{},scale=trunc(iw/2)*2:trunc(ih/2)*2",
            crop.width, crop.height, crop.x, crop.y
        );

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss",
            &format!("{:.3}", start),
            "-i",
            input.to_str().unwrap_or(""),
            "-t",
            &format!("{:.3}", duration),
            "-vf",
            &vf,
            "-c:v",
            "libx264",
            "-preset",
            &self.config.render_preset,
            "-crf",
            &self.config.render_crf.to_string(),
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "FFmpeg segment render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Render segments and concatenate them.
    async fn render_and_concat_segments<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        segments: &[CropSegment],
        base_start: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        // Create temp directory for segments
        let temp_dir = TempDir::new()?;

        let mut segment_files = Vec::new();

        // Render each segment
        for (i, segment) in segments.iter().enumerate() {
            let seg_path = temp_dir.path().join(format!("segment_{:04}.mp4", i));

            self.render_single_segment(input, &seg_path, segment, base_start)
                .await?;

            segment_files.push(seg_path);
        }

        // Create concat list file
        let concat_list_path = temp_dir.path().join("concat.txt");
        let concat_content: String = segment_files
            .iter()
            .map(|p| format!("file '{}'\n", p.display()))
            .collect();

        tokio::fs::write(&concat_list_path, &concat_content).await?;

        // Concatenate segments
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            concat_list_path.to_str().unwrap_or(""),
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg concat: {}", e), None, None)
        })?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "FFmpeg concat failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Compute median crop from windows.
    fn compute_median_crop(&self, windows: &[CropWindow]) -> CropWindow {
        if windows.is_empty() {
            return CropWindow::new(0.0, 0, 0, 1080, 1920);
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

/// A crop segment for rendering.
#[derive(Debug, Clone)]
struct CropSegment {
    /// Start time relative to clip start
    start: f64,
    /// End time relative to clip start
    end: f64,
    /// Crop window for this segment
    crop: CropWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_segments_static() {
        let config = IntelligentCropConfig::default();
        let renderer = IntelligentRenderer::new(config);

        let windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 102, 101, 500, 500),
            CropWindow::new(2.0, 101, 100, 500, 500),
        ];

        let segments = renderer.group_segments(&windows);
        assert_eq!(segments.len(), 1); // Should be one segment
    }

    #[test]
    fn test_group_segments_dynamic() {
        let config = IntelligentCropConfig::default();
        let renderer = IntelligentRenderer::new(config);

        let windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 100, 100, 500, 500),
            CropWindow::new(2.0, 300, 300, 500, 500), // Big change
            CropWindow::new(3.0, 300, 300, 500, 500),
        ];

        let segments = renderer.group_segments(&windows);
        assert_eq!(segments.len(), 2); // Should be two segments
    }

    #[test]
    fn test_median_crop() {
        let config = IntelligentCropConfig::default();
        let renderer = IntelligentRenderer::new(config);

        let windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 200, 200, 600, 600),
            CropWindow::new(2.0, 150, 150, 550, 550),
        ];

        let median = renderer.compute_median_crop(&windows);
        assert_eq!(median.x, 150);
        assert_eq!(median.y, 150);
        assert_eq!(median.width, 550);
    }
}
