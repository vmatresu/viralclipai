//! Shot boundary signal extraction using FFmpeg.
//!
//! Provides histogram-based shot detection without OpenCV dependency.
//! Uses FFmpeg to extract raw RGB frames and compute HSV histograms.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::super::shot_detector::ShotDetector;
use crate::error::{MediaError, MediaResult};

/// A shot boundary with timing information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShotBoundary {
    /// Start time in seconds
    pub start_time: f64,
    /// End time in seconds
    pub end_time: f64,
    /// Start frame index (at sample FPS)
    pub start_frame: usize,
    /// End frame index (at sample FPS)
    pub end_frame: usize,
}

impl ShotBoundary {
    /// Create a new shot boundary.
    pub fn new(start_time: f64, end_time: f64) -> Self {
        Self {
            start_time,
            end_time,
            start_frame: 0,
            end_frame: 0,
        }
    }

    /// Create with frame indices.
    pub fn with_frames(
        start_time: f64,
        end_time: f64,
        start_frame: usize,
        end_frame: usize,
    ) -> Self {
        Self {
            start_time,
            end_time,
            start_frame,
            end_frame,
        }
    }

    /// Duration of the shot in seconds.
    pub fn duration(&self) -> f64 {
        self.end_time - self.start_time
    }
}

/// Shot signal extraction from video.
///
/// Uses FFmpeg to extract frames and compute histograms for shot detection.
pub struct ShotSignals {
    /// Sample FPS for histogram extraction (lower = faster, less accurate)
    sample_fps: f64,
    /// Shot detector with configured threshold
    detector: ShotDetector,
}

impl ShotSignals {
    /// Create with default settings.
    pub fn new() -> Self {
        Self {
            sample_fps: 5.0,
            detector: ShotDetector::new(),
        }
    }

    /// Create with custom settings.
    pub fn with_config(sample_fps: f64, threshold: f64, min_duration: f64) -> Self {
        let min_frames = (min_duration * sample_fps).ceil() as usize;
        Self {
            sample_fps,
            detector: ShotDetector::new()
                .with_threshold(threshold)
                .with_min_frames(min_frames),
        }
    }

    /// Extract shot boundaries from a video segment.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file
    /// * `start_time` - Start time in seconds
    /// * `end_time` - End time in seconds
    ///
    /// # Returns
    /// Vector of detected shot boundaries.
    pub async fn extract<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
    ) -> MediaResult<Vec<ShotBoundary>> {
        let video_path = video_path.as_ref();
        let duration = end_time - start_time;

        if duration <= 0.0 {
            return Ok(vec![ShotBoundary::new(start_time, end_time)]);
        }

        info!(
            "[SHOT_SIGNALS] Extracting histograms from {:?} ({:.2}s-{:.2}s) at {:.1}fps",
            video_path, start_time, end_time, self.sample_fps
        );

        // Extract histograms using FFmpeg
        let histograms = self
            .extract_histograms_ffmpeg(video_path, start_time, end_time)
            .await?;

        if histograms.is_empty() {
            warn!("[SHOT_SIGNALS] No histograms extracted, returning single shot");
            return Ok(vec![ShotBoundary::new(start_time, end_time)]);
        }

        // Detect shots from histograms
        let shots = self
            .detector
            .detect_from_histograms(&histograms, self.sample_fps);

        // Convert to ShotBoundary format with adjusted times
        let boundaries: Vec<ShotBoundary> = shots
            .into_iter()
            .map(|shot| {
                ShotBoundary::with_frames(
                    start_time + shot.start_time,
                    start_time + shot.end_time,
                    shot.start_frame,
                    shot.end_frame,
                )
            })
            .collect();

        info!(
            "[SHOT_SIGNALS] Detected {} shots from {} histogram samples",
            boundaries.len(),
            histograms.len()
        );

        Ok(boundaries)
    }

    /// Extract HSV histograms using FFmpeg rawvideo output.
    async fn extract_histograms_ffmpeg(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
    ) -> MediaResult<Vec<Vec<f64>>> {
        let duration = end_time - start_time;
        let num_samples = (duration * self.sample_fps).ceil() as usize;

        // Small thumbnail resolution for fast histogram computation
        const THUMB_WIDTH: u32 = 160;
        const THUMB_HEIGHT: u32 = 90;
        let bytes_per_frame = (THUMB_WIDTH * THUMB_HEIGHT * 3) as usize;

        // Build FFmpeg command to extract RGB24 frames at sample rate
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.3}", start_time),
            "-t",
            &format!("{:.3}", duration),
            "-i",
        ])
        .arg(video_path)
        .args([
            "-vf",
            &format!(
                "fps={},scale={}:{}",
                self.sample_fps, THUMB_WIDTH, THUMB_HEIGHT
            ),
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        debug!("[SHOT_SIGNALS] Running FFmpeg for histogram extraction");

        let mut child = cmd
            .spawn()
            .map_err(|e| MediaError::ffmpeg_failed(format!("Failed to spawn FFmpeg: {}", e), None, None))?;

        let stdout = child.stdout.take().ok_or_else(|| {
            MediaError::ffmpeg_failed("Failed to capture FFmpeg stdout", None, None)
        })?;

        // Read all raw video data
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut buffer = Vec::with_capacity(num_samples * bytes_per_frame);
        reader.read_to_end(&mut buffer).await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to read FFmpeg output: {}", e), None, None)
        })?;

        // Wait for FFmpeg to complete
        let status = child.wait().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("FFmpeg process error: {}", e), None, None)
        })?;

        if !status.success() {
            warn!(
                "[SHOT_SIGNALS] FFmpeg returned non-zero status: {:?}",
                status.code()
            );
        }

        // Compute histogram for each frame
        let actual_frames = buffer.len() / bytes_per_frame;
        debug!(
            "[SHOT_SIGNALS] Extracted {} frames ({} bytes)",
            actual_frames,
            buffer.len()
        );

        let mut histograms = Vec::with_capacity(actual_frames);
        for i in 0..actual_frames {
            let frame_start = i * bytes_per_frame;
            let frame_end = frame_start + bytes_per_frame;
            let frame_data = &buffer[frame_start..frame_end];

            let histogram =
                self.detector
                    .compute_histogram(frame_data, THUMB_WIDTH, THUMB_HEIGHT);
            histograms.push(histogram);
        }

        Ok(histograms)
    }
}

impl Default for ShotSignals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shot_boundary_creation() {
        let boundary = ShotBoundary::new(1.0, 5.0);
        assert!((boundary.duration() - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_shot_boundary_with_frames() {
        let boundary = ShotBoundary::with_frames(0.0, 2.0, 0, 59);
        assert_eq!(boundary.start_frame, 0);
        assert_eq!(boundary.end_frame, 59);
    }

    #[test]
    fn test_shot_signals_config() {
        let signals = ShotSignals::with_config(10.0, 0.4, 1.0);
        assert!((signals.sample_fps - 10.0).abs() < 0.001);
    }
}
