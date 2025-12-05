//! FFmpeg command builder and runner.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::error::{MediaError, MediaResult};
use crate::progress::FfmpegProgress;

/// Builder for FFmpeg commands.
#[derive(Debug, Clone)]
pub struct FfmpegCommand {
    /// Input file path
    input: PathBuf,
    /// Output file path
    output: PathBuf,
    /// Input arguments (before -i)
    input_args: Vec<String>,
    /// Output arguments (after -i)
    output_args: Vec<String>,
    /// Whether to overwrite output
    overwrite: bool,
    /// Log level
    log_level: String,
}

impl FfmpegCommand {
    /// Create a new FFmpeg command.
    pub fn new(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            output: output.as_ref().to_path_buf(),
            input_args: Vec::new(),
            output_args: Vec::new(),
            overwrite: true,
            log_level: "error".to_string(),
        }
    }

    /// Add input arguments (before -i).
    pub fn input_arg(mut self, arg: impl Into<String>) -> Self {
        self.input_args.push(arg.into());
        self
    }

    /// Add multiple input arguments.
    pub fn input_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.input_args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Add output arguments (after -i).
    pub fn output_arg(mut self, arg: impl Into<String>) -> Self {
        self.output_args.push(arg.into());
        self
    }

    /// Add multiple output arguments.
    pub fn output_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.output_args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set seek position (before input).
    pub fn seek(self, seconds: f64) -> Self {
        self.input_arg("-ss").input_arg(format!("{:.3}", seconds))
    }

    /// Set duration.
    pub fn duration(self, seconds: f64) -> Self {
        self.input_arg("-t").input_arg(format!("{:.3}", seconds))
    }

    /// Set video filter.
    pub fn video_filter(self, filter: impl Into<String>) -> Self {
        self.output_arg("-vf").output_arg(filter)
    }

    /// Set filter complex.
    pub fn filter_complex(self, filter: impl Into<String>) -> Self {
        self.output_arg("-filter_complex").output_arg(filter)
    }

    /// Set video codec.
    pub fn video_codec(self, codec: impl Into<String>) -> Self {
        self.output_arg("-c:v").output_arg(codec)
    }

    /// Set audio codec.
    pub fn audio_codec(self, codec: impl Into<String>) -> Self {
        self.output_arg("-c:a").output_arg(codec)
    }

    /// Set CRF (quality).
    pub fn crf(self, crf: u8) -> Self {
        self.output_arg("-crf").output_arg(crf.to_string())
    }

    /// Set preset.
    pub fn preset(self, preset: impl Into<String>) -> Self {
        self.output_arg("-preset").output_arg(preset)
    }

    /// Set audio bitrate.
    pub fn audio_bitrate(self, bitrate: impl Into<String>) -> Self {
        self.output_arg("-b:a").output_arg(bitrate)
    }

    /// Extract single frame.
    pub fn single_frame(self) -> Self {
        self.output_arg("-vframes").output_arg("1")
    }

    /// Set log level.
    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Build the command arguments.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Overwrite flag
        if self.overwrite {
            args.push("-y".to_string());
        }

        // Log level
        args.push("-v".to_string());
        args.push(self.log_level.clone());

        // Progress output to stderr
        args.push("-progress".to_string());
        args.push("pipe:2".to_string());

        // Input args
        args.extend(self.input_args.clone());

        // Input file
        args.push("-i".to_string());
        args.push(self.input.to_string_lossy().to_string());

        // Output args
        args.extend(self.output_args.clone());

        // Output file
        args.push(self.output.to_string_lossy().to_string());

        args
    }
}

/// Runner for FFmpeg commands with progress tracking and cancellation.
pub struct FfmpegRunner {
    /// Cancellation signal receiver
    cancel_rx: Option<watch::Receiver<bool>>,
    /// Timeout in seconds
    timeout_secs: Option<u64>,
}

impl Default for FfmpegRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl FfmpegRunner {
    /// Create a new runner.
    pub fn new() -> Self {
        Self {
            cancel_rx: None,
            timeout_secs: None,
        }
    }

    /// Set cancellation signal.
    pub fn with_cancel(mut self, cancel_rx: watch::Receiver<bool>) -> Self {
        self.cancel_rx = Some(cancel_rx);
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Run an FFmpeg command.
    pub async fn run(&self, cmd: &FfmpegCommand) -> MediaResult<()> {
        self.run_with_progress(cmd, |_| {}).await
    }

    /// Run an FFmpeg command with progress callback.
    pub async fn run_with_progress<F>(&self, cmd: &FfmpegCommand, progress_callback: F) -> MediaResult<()>
    where
        F: Fn(FfmpegProgress) + Send + 'static,
    {
        // Check FFmpeg exists
        which::which("ffmpeg").map_err(|_| MediaError::FfmpegNotFound)?;

        let args = cmd.build_args();
        debug!("Running FFmpeg: ffmpeg {}", args.join(" "));

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stderr = child.stderr.take().expect("stderr not captured");
        let mut reader = BufReader::new(stderr).lines();

        // Spawn progress parsing task
        let progress_handle = tokio::spawn(async move {
            let mut current_progress = FfmpegProgress::default();

            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(progress) = parse_progress_line(&line, &mut current_progress) {
                    progress_callback(progress.clone());
                }
            }
        });

        // Wait for completion with optional timeout and cancellation
        let result = self.wait_for_completion(&mut child).await;

        // Wait for progress task to complete
        let _ = progress_handle.await;

        result
    }

    /// Wait for child process with cancellation and timeout.
    async fn wait_for_completion(&self, child: &mut Child) -> MediaResult<()> {
        let wait_future = child.wait();

        // Apply timeout if set
        let wait_future = if let Some(timeout_secs) = self.timeout_secs {
            let timeout = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                wait_future,
            );
            match timeout.await {
                Ok(result) => result,
                Err(_) => {
                    // Timeout - kill the process
                    warn!("FFmpeg timed out after {} seconds, killing process", timeout_secs);
                    let _ = child.kill().await;
                    return Err(MediaError::Timeout(timeout_secs));
                }
            }
        } else {
            wait_future.await
        };

        // Check cancellation
        if let Some(ref cancel_rx) = self.cancel_rx {
            if *cancel_rx.borrow() {
                info!("FFmpeg cancelled, killing process");
                let _ = child.kill().await;
                return Err(MediaError::Cancelled);
            }
        }

        let status = wait_future?;

        if status.success() {
            Ok(())
        } else {
            Err(MediaError::ffmpeg_failed(
                "FFmpeg exited with non-zero status",
                None,
                status.code(),
            ))
        }
    }
}

/// Parse a progress line from FFmpeg's -progress output.
fn parse_progress_line(line: &str, current: &mut FfmpegProgress) -> Option<FfmpegProgress> {
    let line = line.trim();

    if let Some((key, value)) = line.split_once('=') {
        match key {
            "out_time_ms" | "out_time_us" => {
                // Parse microseconds or milliseconds to milliseconds
                if let Ok(us) = value.parse::<i64>() {
                    current.out_time_ms = if key == "out_time_us" {
                        us / 1000
                    } else {
                        us
                    };
                }
            }
            "out_time" => {
                // Format: HH:MM:SS.microseconds
                current.out_time = value.to_string();
            }
            "frame" => {
                if let Ok(frame) = value.parse() {
                    current.frame = frame;
                }
            }
            "fps" => {
                if let Ok(fps) = value.parse() {
                    current.fps = fps;
                }
            }
            "speed" => {
                // Format: "1.5x" or "N/A"
                if value != "N/A" {
                    if let Some(speed_str) = value.strip_suffix('x') {
                        if let Ok(speed) = speed_str.parse() {
                            current.speed = speed;
                        }
                    }
                }
            }
            "progress" => {
                // "continue" or "end"
                if value == "end" {
                    current.is_complete = true;
                }
                return Some(current.clone());
            }
            _ => {}
        }
    }

    None
}

/// Check if FFmpeg is available.
pub fn check_ffmpeg() -> MediaResult<PathBuf> {
    which::which("ffmpeg").map_err(|_| MediaError::FfmpegNotFound)
}

/// Check if FFprobe is available.
pub fn check_ffprobe() -> MediaResult<PathBuf> {
    which::which("ffprobe").map_err(|_| MediaError::FfprobeNotFound)
}

/// Check if yt-dlp is available.
pub fn check_ytdlp() -> MediaResult<PathBuf> {
    which::which("yt-dlp").map_err(|_| MediaError::YtDlpNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_builder() {
        let cmd = FfmpegCommand::new("input.mp4", "output.mp4")
            .seek(10.0)
            .duration(30.0)
            .video_codec("libx264")
            .crf(18);

        let args = cmd.build_args();
        assert!(args.contains(&"-ss".to_string()));
        assert!(args.contains(&"10.000".to_string()));
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
    }

    #[test]
    fn test_progress_parsing() {
        let mut progress = FfmpegProgress::default();

        parse_progress_line("out_time_ms=5000000", &mut progress);
        assert_eq!(progress.out_time_ms, 5000000);

        parse_progress_line("speed=1.5x", &mut progress);
        assert!((progress.speed - 1.5).abs() < 0.01);

        let result = parse_progress_line("progress=end", &mut progress);
        assert!(result.is_some());
        assert!(progress.is_complete);
    }
}
