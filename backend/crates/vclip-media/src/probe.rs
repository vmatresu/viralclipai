//! FFprobe video information.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::error::{MediaError, MediaResult};

/// Video file information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// Duration in seconds
    pub duration: f64,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Frame rate (fps)
    pub fps: f64,
    /// Video codec
    pub codec: String,
    /// File size in bytes
    pub size: u64,
    /// Bitrate in bits/second
    pub bitrate: u64,
}

/// FFprobe JSON output format.
#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: FfprobeFormat,
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
}

/// Probe a video file for information.
pub async fn probe_video(path: impl AsRef<Path>) -> MediaResult<VideoInfo> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(MediaError::FileNotFound(path.to_path_buf()));
    }

    // Check FFprobe exists
    which::which("ffprobe").map_err(|_| MediaError::FfprobeNotFound)?;

    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        return Err(MediaError::FfprobeFailed {
            message: "FFprobe failed".to_string(),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        });
    }

    let probe: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    // Find video stream
    let video_stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .ok_or_else(|| MediaError::InvalidVideo("No video stream found".to_string()))?;

    // Parse duration
    let duration = probe
        .format
        .duration
        .as_ref()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Parse size
    let size = probe
        .format
        .size
        .as_ref()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Parse bitrate
    let bitrate = probe
        .format
        .bit_rate
        .as_ref()
        .and_then(|b| b.parse::<u64>().ok())
        .unwrap_or(0);

    // Parse frame rate
    let fps = video_stream
        .avg_frame_rate
        .as_ref()
        .or(video_stream.r_frame_rate.as_ref())
        .and_then(|r| parse_frame_rate(r))
        .unwrap_or(30.0);

    Ok(VideoInfo {
        duration,
        width: video_stream.width.unwrap_or(0),
        height: video_stream.height.unwrap_or(0),
        fps,
        codec: video_stream.codec_name.clone().unwrap_or_default(),
        size,
        bitrate,
    })
}

/// Get video duration in seconds.
pub async fn get_duration(path: impl AsRef<Path>) -> MediaResult<f64> {
    let info = probe_video(path).await?;
    Ok(info.duration)
}

/// Parse frame rate string (e.g., "30/1" or "29.97").
fn parse_frame_rate(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let num: f64 = num.parse().ok()?;
        let den: f64 = den.parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frame_rate() {
        assert!((parse_frame_rate("30/1").unwrap() - 30.0).abs() < 0.01);
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert!((parse_frame_rate("29.97").unwrap() - 29.97).abs() < 0.01);
    }
}
