//! Continuous FFmpeg filter renderer for seamless video processing.
//!
//! This module provides a single-pass rendering approach that eliminates
//! PTS discontinuities and visual artifacts when switching between layouts.
//!
//! Key features:
//! - **Single filter graph**: No segment concatenation needed
//! - **Dynamic crop expressions**: Uses FFmpeg's sendcmd or expression evaluation
//! - **Continuous overlay**: Split views without stream switching artifacts
//! - **Proper SAR/DAR handling**: Normalized pixel aspect ratios

use super::config::IntelligentCropConfig;
use super::models::CropWindow;
use super::output_format::{PORTRAIT_WIDTH, PORTRAIT_HEIGHT, SPLIT_PANEL_WIDTH, SPLIT_PANEL_HEIGHT};
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

/// Continuous renderer that uses a single FFmpeg filter graph.
///
/// Instead of rendering segments separately and concatenating,
/// this renderer builds a complex filter graph that handles
/// all crop transitions within a single pass.
pub struct ContinuousRenderer {
    config: IntelligentCropConfig,
}

impl ContinuousRenderer {
    /// Create a new continuous renderer.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self { config }
    }

    /// Render with dynamic cropping using sendcmd filter.
    ///
    /// This approach sends crop parameter changes at specific timestamps,
    /// avoiding the need to concatenate separately encoded segments.
    pub async fn render_dynamic<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        crop_windows: &[CropWindow],
        start_time: f64,
        duration: f64,
        output_width: u32,
        output_height: u32,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if crop_windows.is_empty() {
            return Err(MediaError::InvalidVideo("No crop windows provided".to_string()));
        }

        info!(
            "Continuous render: {} crop windows, {}s duration, output {}x{}",
            crop_windows.len(),
            duration,
            output_width,
            output_height
        );

        // Build the filter graph
        let filter_complex = self.build_dynamic_crop_filter(
            crop_windows,
            start_time,
            output_width,
            output_height,
        );

        debug!("Filter graph:\n{}", filter_complex);

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss", &format!("{:.3}", start_time),
            "-i", input.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-filter_complex", &filter_complex,
            "-map", "[vout]",
            "-map", "0:a?",
            // Video encoding with consistent settings
            "-c:v", "libx264",
            "-preset", &self.config.render_preset,
            "-crf", &self.config.render_crf.to_string(),
            "-pix_fmt", "yuv420p",
            // Ensure consistent timestamps
            "-vsync", "cfr",
            "-video_track_timescale", "90000",
            // Audio
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
            return Err(MediaError::ffmpeg_failed(
                "Continuous render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Build a dynamic crop filter using sendcmd for parameter updates.
    ///
    /// Uses the zoompan filter for smooth interpolated crop transitions.
    fn build_dynamic_crop_filter(
        &self,
        crop_windows: &[CropWindow],
        _start_time: f64,
        output_width: u32,
        output_height: u32,
    ) -> String {
        // Group crop windows into segments for sendcmd
        let segments = self.group_crop_segments(crop_windows);

        // Build sendcmd script for crop parameter changes
        let sendcmd_script = self.build_sendcmd_script(&segments);

        // Use a format with normalized SAR to prevent aspect ratio issues
        format!(
            "[0:v]setsar=1,format=yuv420p,\
             sendcmd=f='{sendcmd_script}',\
             crop@dyncrop=w={initial_w}:h={initial_h}:x={initial_x}:y={initial_y}:exact=1,\
             scale={out_w}:{out_h}:flags=lanczos,\
             setsar=1[vout]",
            sendcmd_script = sendcmd_script,
            initial_w = crop_windows[0].width,
            initial_h = crop_windows[0].height,
            initial_x = crop_windows[0].x,
            initial_y = crop_windows[0].y,
            out_w = output_width,
            out_h = output_height,
        )
    }

    /// Group crop windows into segments with the same crop parameters.
    fn group_crop_segments(&self, windows: &[CropWindow]) -> Vec<CropSegment> {
        if windows.is_empty() {
            return Vec::new();
        }

        let tolerance = 5; // 5px tolerance
        let mut segments = Vec::new();
        let mut current_start = windows[0].time;
        let mut current_crop = windows[0];

        for window in windows.iter().skip(1) {
            if self.crops_differ_by(&current_crop, window, tolerance) {
                segments.push(CropSegment {
                    start_time: current_start,
                    crop: current_crop,
                });
                current_start = window.time;
                current_crop = *window;
            }
        }

        // Final segment
        if !windows.is_empty() {
            segments.push(CropSegment {
                start_time: current_start,
                crop: current_crop,
            });
        }

        segments
    }

    /// Check if two crops differ by more than tolerance pixels.
    fn crops_differ_by(&self, a: &CropWindow, b: &CropWindow, tolerance: i32) -> bool {
        (a.x - b.x).abs() > tolerance
            || (a.y - b.y).abs() > tolerance
            || (a.width - b.width).abs() > tolerance
            || (a.height - b.height).abs() > tolerance
    }

    /// Build sendcmd script for crop parameter updates.
    ///
    /// Format: `timestamp [enter|leave] command;`
    fn build_sendcmd_script(&self, segments: &[CropSegment]) -> String {
        let mut commands = Vec::new();

        for segment in segments {
            // Set crop parameters at segment start
            let time = segment.start_time;
            let crop = &segment.crop;

            // sendcmd format: time command
            // Use reinit to smoothly transition crop parameters
            commands.push(format!(
                "{:.3} [enter] crop@dyncrop w {}, crop@dyncrop h {}, crop@dyncrop x {}, crop@dyncrop y {}",
                time, crop.width, crop.height, crop.x, crop.y
            ));
        }

        commands.join("; ")
    }

    /// Render a split view with continuous overlays.
    ///
    /// Instead of rendering separate clips and stacking, this creates
    /// a single filter graph with two crop operations and vstack.
    pub async fn render_split_continuous<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        left_crops: &[CropWindow],
        right_crops: &[CropWindow],
        start_time: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if left_crops.is_empty() || right_crops.is_empty() {
            return Err(MediaError::InvalidVideo("Empty crop windows for split view".to_string()));
        }

        info!(
            "Continuous split render: {} left crops, {} right crops, {}s duration",
            left_crops.len(),
            right_crops.len(),
            duration
        );

        let filter_complex = self.build_split_filter(left_crops, right_crops);

        debug!("Split filter graph:\n{}", filter_complex);

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss", &format!("{:.3}", start_time),
            "-i", input.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-filter_complex", &filter_complex,
            "-map", "[vout]",
            "-map", "0:a?",
            // Consistent encoding
            "-c:v", "libx264",
            "-preset", &self.config.render_preset,
            "-crf", &self.config.render_crf.to_string(),
            "-pix_fmt", "yuv420p",
            // Timestamp normalization - critical for avoiding PTS glitches
            "-vsync", "cfr",
            "-video_track_timescale", "90000",
            // Audio with resample to fix any discontinuities
            "-c:a", "aac",
            "-b:a", "128k",
            "-af", "aresample=async=1:first_pts=0",
            // Finalize
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
            return Err(MediaError::ffmpeg_failed(
                "Split render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Build a split-view filter graph with two continuous crop paths.
    ///
    /// Uses split filter to create two branches, each with its own crop,
    /// then vstacks them together. All in one filter graph.
    fn build_split_filter(&self, left_crops: &[CropWindow], right_crops: &[CropWindow]) -> String {
        // Use median crop for each panel (static split) or first crop
        let left_crop = self.compute_median_crop(left_crops);
        let right_crop = self.compute_median_crop(right_crops);

        // Use centralized panel dimensions for consistent 9:16 output
        let panel_width = SPLIT_PANEL_WIDTH;
        let panel_height = SPLIT_PANEL_HEIGHT;

        format!(
            // Normalize input
            "[0:v]setsar=1,setpts=PTS-STARTPTS,format=yuv420p[base];\
             \
             [base]split=2[left_in][right_in];\
             \
             [left_in]crop={left_w}:{left_h}:{left_x}:{left_y}:exact=1,\
             scale={pw}:{ph}:force_original_aspect_ratio=decrease,\
             pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,\
             setsar=1[top];\
             \
             [right_in]crop={right_w}:{right_h}:{right_x}:{right_y}:exact=1,\
             scale={pw}:{ph}:force_original_aspect_ratio=decrease,\
             pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,\
             setsar=1[bottom];\
             \
             [top][bottom]vstack=inputs=2[vout]",
            // Left crop
            left_w = left_crop.width,
            left_h = left_crop.height,
            left_x = left_crop.x,
            left_y = left_crop.y,
            // Right crop
            right_w = right_crop.width,
            right_h = right_crop.height,
            right_x = right_crop.x,
            right_y = right_crop.y,
            // Panel size
            pw = panel_width,
            ph = panel_height,
        )
    }

    /// Render with hybrid layout switching (Full ↔ Split).
    ///
    /// Uses overlay with enable expressions to smoothly transition
    /// between full-screen and split-screen layouts without PTS issues.
    pub async fn render_hybrid_layout<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        layout_spans: &[LayoutSpan],
        full_crops: &[CropWindow],
        left_crops: &[CropWindow],
        right_crops: &[CropWindow],
        start_time: f64,
        duration: f64,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if layout_spans.is_empty() {
            return Err(MediaError::InvalidVideo("No layout spans provided".to_string()));
        }

        info!(
            "Hybrid layout render: {} spans, {}s duration",
            layout_spans.len(),
            duration
        );

        let filter_complex = self.build_hybrid_filter(
            layout_spans,
            full_crops,
            left_crops,
            right_crops,
            duration,
        );

        debug!("Hybrid filter graph:\n{}", filter_complex);

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-ss", &format!("{:.3}", start_time),
            "-i", input.to_str().unwrap_or(""),
            "-t", &format!("{:.3}", duration),
            "-filter_complex", &filter_complex,
            "-map", "[vout]",
            "-map", "0:a?",
            // Consistent encoding
            "-c:v", "libx264",
            "-preset", &self.config.render_preset,
            "-crf", &self.config.render_crf.to_string(),
            "-pix_fmt", "yuv420p",
            // Critical: Constant frame rate for seamless playback
            "-vsync", "cfr",
            "-video_track_timescale", "90000",
            // Audio
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
            return Err(MediaError::ffmpeg_failed(
                "Hybrid layout render failed",
                Some(stderr.to_string()),
                output_result.status.code(),
            ));
        }

        Ok(())
    }

    /// Build a hybrid filter that switches between full and split layouts.
    ///
    /// Uses overlay with `enable` expression to toggle between:
    /// - Full view: Single crop scaled to 1080x1920
    /// - Split view: Two crops stacked vertically
    fn build_hybrid_filter(
        &self,
        layout_spans: &[LayoutSpan],
        full_crops: &[CropWindow],
        left_crops: &[CropWindow],
        right_crops: &[CropWindow],
        _duration: f64,
    ) -> String {
        let full_crop = self.compute_median_crop(full_crops);
        let left_crop = self.compute_median_crop(left_crops);
        let right_crop = self.compute_median_crop(right_crops);

        // Build enable expressions for each layout type
        let full_enable = self.build_enable_expression(layout_spans, LayoutType::Full);
        let split_enable = self.build_enable_expression(layout_spans, LayoutType::Split);

        // Use centralized dimensions for consistent output
        let panel_width = SPLIT_PANEL_WIDTH;
        let panel_height = SPLIT_PANEL_HEIGHT;
        let output_height = PORTRAIT_HEIGHT;

        format!(
            // Input normalization with PTS reset
            "[0:v]setsar=1,setpts=PTS-STARTPTS,format=yuv420p[base];\
             \
             [base]split=4[full_in][left_in][right_in][canvas_src];\
             \
             [canvas_src]scale={pw}:{oh},drawbox=c=black@1:t=fill[canvas];\
             \
             [full_in]crop={full_w}:{full_h}:{full_x}:{full_y}:exact=1,\
             scale={pw}:{oh}:force_original_aspect_ratio=decrease,\
             pad={pw}:{oh}:(ow-iw)/2:(oh-ih)/2,\
             setsar=1[full_scaled];\
             \
             [left_in]crop={left_w}:{left_h}:{left_x}:{left_y}:exact=1,\
             scale={pw}:{ph}:force_original_aspect_ratio=decrease,\
             pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,setsar=1[top_panel];\
             \
             [right_in]crop={right_w}:{right_h}:{right_x}:{right_y}:exact=1,\
             scale={pw}:{ph}:force_original_aspect_ratio=decrease,\
             pad={pw}:{ph}:(ow-iw)/2:(oh-ih)/2,setsar=1[bottom_panel];\
             \
             [top_panel][bottom_panel]vstack=inputs=2[split_view];\
             \
             [canvas][full_scaled]overlay=0:0:enable='{full_enable}'[with_full];\
             [with_full][split_view]overlay=0:0:enable='{split_enable}'[vout]",
            // Dimensions
            pw = panel_width,
            ph = panel_height,
            oh = output_height,
            // Full view crop
            full_w = full_crop.width,
            full_h = full_crop.height,
            full_x = full_crop.x,
            full_y = full_crop.y,
            // Left crop
            left_w = left_crop.width,
            left_h = left_crop.height,
            left_x = left_crop.x,
            left_y = left_crop.y,
            // Right crop
            right_w = right_crop.width,
            right_h = right_crop.height,
            right_x = right_crop.x,
            right_y = right_crop.y,
            // Enable expressions
            full_enable = full_enable,
            split_enable = split_enable,
        )
    }

    /// Build FFmpeg enable expression for layout type.
    ///
    /// Format: `between(t,start1,end1)+between(t,start2,end2)+...`
    fn build_enable_expression(&self, spans: &[LayoutSpan], layout_type: LayoutType) -> String {
        let matching_spans: Vec<_> = spans
            .iter()
            .filter(|s| s.layout_type == layout_type)
            .collect();

        if matching_spans.is_empty() {
            return "0".to_string(); // Never enable
        }

        matching_spans
            .iter()
            .map(|s| format!("between(t,{:.3},{:.3})", s.start, s.end))
            .collect::<Vec<_>>()
            .join("+")
    }

    /// Compute median crop from windows.
    fn compute_median_crop(&self, windows: &[CropWindow]) -> CropWindow {
        if windows.is_empty() {
            return CropWindow::new(0.0, 0, 0, PORTRAIT_WIDTH as i32, PORTRAIT_HEIGHT as i32);
        }

        if windows.len() == 1 {
            return windows[0];
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

/// A segment with consistent crop parameters.
#[derive(Debug, Clone)]
struct CropSegment {
    start_time: f64,
    crop: CropWindow,
}

/// Layout type for hybrid rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutType {
    /// Full-screen single speaker view
    Full,
    /// Split-screen two-speaker view
    Split,
}

/// A time span with a specific layout.
#[derive(Debug, Clone)]
pub struct LayoutSpan {
    /// Start time in seconds
    pub start: f64,
    /// End time in seconds
    pub end: f64,
    /// Layout type for this span
    pub layout_type: LayoutType,
}

impl LayoutSpan {
    /// Create a new layout span.
    pub fn new(start: f64, end: f64, layout_type: LayoutType) -> Self {
        Self { start, end, layout_type }
    }

    /// Create a full-view span.
    pub fn full(start: f64, end: f64) -> Self {
        Self::new(start, end, LayoutType::Full)
    }

    /// Create a split-view span.
    pub fn split(start: f64, end: f64) -> Self {
        Self::new(start, end, LayoutType::Split)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_enable_expression() {
        let config = IntelligentCropConfig::default();
        let renderer = ContinuousRenderer::new(config);

        let spans = vec![
            LayoutSpan::full(0.0, 5.0),
            LayoutSpan::split(5.0, 10.0),
            LayoutSpan::full(10.0, 15.0),
        ];

        let full_expr = renderer.build_enable_expression(&spans, LayoutType::Full);
        assert!(full_expr.contains("between(t,0.000,5.000)"));
        assert!(full_expr.contains("between(t,10.000,15.000)"));

        let split_expr = renderer.build_enable_expression(&spans, LayoutType::Split);
        assert!(split_expr.contains("between(t,5.000,10.000)"));
    }

    #[test]
    fn test_group_crop_segments() {
        let config = IntelligentCropConfig::default();
        let renderer = ContinuousRenderer::new(config);

        let windows = vec![
            CropWindow::new(0.0, 100, 100, 500, 500),
            CropWindow::new(1.0, 102, 101, 500, 500), // Within tolerance
            CropWindow::new(2.0, 300, 300, 500, 500), // Big jump
            CropWindow::new(3.0, 301, 301, 500, 500), // Within tolerance
        ];

        let segments = renderer.group_crop_segments(&windows);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].crop.x, 100);
        assert_eq!(segments[1].crop.x, 300);
    }

    #[test]
    fn test_sendcmd_script() {
        let config = IntelligentCropConfig::default();
        let renderer = ContinuousRenderer::new(config);

        let segments = vec![
            CropSegment {
                start_time: 0.0,
                end_time: 5.0,
                crop: CropWindow::new(0.0, 100, 100, 500, 500),
            },
            CropSegment {
                start_time: 5.0,
                end_time: 10.0,
                crop: CropWindow::new(5.0, 200, 200, 600, 600),
            },
        ];

        let script = renderer.build_sendcmd_script(&segments);
        assert!(script.contains("0.000 [enter]"));
        assert!(script.contains("5.000 [enter]"));
        assert!(script.contains("crop@dyncrop w 500"));
        assert!(script.contains("crop@dyncrop w 600"));
    }
}
