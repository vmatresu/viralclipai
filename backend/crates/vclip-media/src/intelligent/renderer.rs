//! FFmpeg-based rendering of intelligent cropped videos.
//!
//! Converts crop windows into FFmpeg commands for final rendering.

use super::config::IntelligentCropConfig;
use super::crop_planner::is_static_crop;
use super::models::CropWindow;
use super::output_format::{PORTRAIT_WIDTH, PORTRAIT_HEIGHT};
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, info};

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
        // Crop to 9:16 region, then scale to exact 1080×1920 portrait output
        let vf = format!(
            "crop={}:{}:{}:{},scale={}:{}:flags=lanczos,setsar=1",
            crop.width, crop.height, crop.x, crop.y,
            PORTRAIT_WIDTH, PORTRAIT_HEIGHT
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

    /// Render with dynamic cropping using continuous filter graph.
    ///
    /// **Key fix for PTS discontinuity (garbled flash)**: Uses a single-pass
    /// filter graph with sendcmd to update crop parameters dynamically,
    /// instead of rendering segments separately and concatenating.
    async fn render_dynamic_crop<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        start_time: f64,
        duration: f64,
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

        info!(
            "Continuous dynamic crop: {} segments over {:.2}s",
            segments.len(),
            duration
        );

        // Use continuous rendering to avoid PTS discontinuities
        self.render_continuous_dynamic(input, output, crop_windows, &segments, start_time, duration)
            .await
    }

    /// Render using continuous filter graph (no segment concatenation).
    ///
    /// This method uses sendcmd to dynamically update crop parameters,
    /// keeping timestamps continuous and avoiding decode errors.
    async fn render_continuous_dynamic<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        segments: &[CropSegment],
        start_time: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if segments.is_empty() || crop_windows.is_empty() {
            return Err(MediaError::InvalidVideo("No crop segments provided".to_string()));
        }

        // Build sendcmd script for crop updates
        let sendcmd_script = self.build_sendcmd_script(segments);
        let initial = &segments[0].crop;

        // Build filter graph with sendcmd for dynamic crop updates
        // Key elements:
        // 1. setsar=1 - Normalize pixel aspect ratio
        // 2. setpts=PTS-STARTPTS - Reset timestamps to avoid discontinuities
        // 3. sendcmd - Dynamically update crop parameters
        // 4. format=yuv420p - Ensure compatible pixel format
        // Build filter graph with sendcmd for dynamic crop updates
        // Final scale ensures exact 1080×1920 portrait output
        let filter_complex = format!(
            "[0:v]setsar=1,setpts=PTS-STARTPTS,format=yuv420p,\
             sendcmd=f='{script}',\
             crop@dyncrop=w={w}:h={h}:x={x}:y={y}:exact=1,\
             scale={out_w}:{out_h}:flags=lanczos,setsar=1[vout]",
            script = sendcmd_script,
            w = initial.width,
            h = initial.height,
            x = initial.x,
            y = initial.y,
            out_w = PORTRAIT_WIDTH,
            out_h = PORTRAIT_HEIGHT,
        );

        debug!("Continuous crop filter:\n{}", filter_complex);

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss", &format!("{:.3}", start_time),
            "-i", input.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-filter_complex", &filter_complex,
            "-map", "[vout]",
            "-map", "0:a?",
            // Video encoding
            "-c:v", "libx264",
            "-preset", &self.config.render_preset,
            "-crf", &self.config.render_crf.to_string(),
            "-pix_fmt", "yuv420p",
            // Constant frame rate to prevent timing issues
            "-vsync", "cfr",
            "-video_track_timescale", "90000",
            // Audio with resample to handle any discontinuities
            "-c:a", "aac",
            "-b:a", "128k",
            "-af", "aresample=async=1:first_pts=0",
            // Output
            "-movflags", "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            
            // Fall back to legacy concat if sendcmd not supported
            if stderr.contains("sendcmd") || stderr.contains("Unknown filter") {
                debug!("sendcmd filter not available, falling back to segment concat");
                return self.render_and_concat_segments_fixed(input, output, segments, start_time).await;
            }
            
            return Err(MediaError::ffmpeg_failed(
                "Continuous crop render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Build sendcmd script for dynamic crop updates.
    fn build_sendcmd_script(&self, segments: &[CropSegment]) -> String {
        segments
            .iter()
            .map(|seg| {
                format!(
                    "{:.3} [enter] crop@dyncrop w {}, crop@dyncrop h {}, crop@dyncrop x {}, crop@dyncrop y {}",
                    seg.start, seg.crop.width, seg.crop.height, seg.crop.x, seg.crop.y
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Fallback: Render segments and concatenate with fixed timestamps.
    ///
    /// Uses setpts=PTS-STARTPTS on each segment to ensure clean concatenation.
    async fn render_and_concat_segments_fixed<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        segments: &[CropSegment],
        base_start: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        let temp_dir = TempDir::new()?;
        let mut segment_files = Vec::new();

        // Render each segment with timestamp normalization
        for (i, segment) in segments.iter().enumerate() {
            let seg_path = temp_dir.path().join(format!("segment_{:04}.mp4", i));
            self.render_single_segment_normalized(input, &seg_path, segment, base_start).await?;
            segment_files.push(seg_path);
        }

        // Concatenate with stream copy (segments are now normalized)
        let concat_list_path = temp_dir.path().join("concat.txt");
        let concat_content: String = segment_files
            .iter()
            .map(|p| format!("file '{}'\n", p.display()))
            .collect();

        tokio::fs::write(&concat_list_path, &concat_content).await?;

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-f", "concat",
            "-safe", "0",
            "-i", concat_list_path.to_str().unwrap_or(""),
            // Stream copy - segments are already encoded with normalized timestamps
            "-c:v", "copy",
            "-c:a", "copy",
            "-movflags", "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("FFmpeg concat failed: {}", e), None, None)
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

    /// Render a segment with PTS normalization for clean concatenation.
    async fn render_single_segment_normalized<P: AsRef<Path>>(
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

        // Key fix: setpts=PTS-STARTPTS normalizes timestamps
        // Scale to exact 1080×1920 portrait output with square pixels
        let vf = format!(
            "crop={}:{}:{}:{},setpts=PTS-STARTPTS,scale={}:{}:flags=lanczos,setsar=1",
            crop.width, crop.height, crop.x, crop.y,
            PORTRAIT_WIDTH, PORTRAIT_HEIGHT
        );

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss", &format!("{:.3}", start),
            "-i", input.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-vf", &vf,
            // Audio timestamp normalization
            "-af", "aresample=async=1:first_pts=0",
            "-c:v", "libx264",
            "-preset", &self.config.render_preset,
            "-crf", &self.config.render_crf.to_string(),
            "-c:a", "aac",
            "-b:a", "128k",
            "-pix_fmt", "yuv420p",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output_result = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to render segment: {}", e), None, None)
        })?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            return Err(MediaError::ffmpeg_failed(
                "Segment render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
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

        // Crop to 9:16 region, then scale to exact 1080×1920 portrait output
        let vf = format!(
            "crop={}:{}:{}:{},scale={}:{}:flags=lanczos,setsar=1",
            crop.width, crop.height, crop.x, crop.y,
            PORTRAIT_WIDTH, PORTRAIT_HEIGHT
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

    /// Compute median crop from windows.
    fn compute_median_crop(&self, windows: &[CropWindow]) -> CropWindow {
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
