//! Video clipping operations.
//!
//! # Architecture
//!
//! This module provides three main entry points for clip creation:
//!
//! ## 1. `create_clip()` - Traditional Styles
//! Handles: `Original`, `Split`, `LeftFocus`, `RightFocus`
//! - Uses FFmpeg video filters for transformations
//! - Single-pass processing
//! - Fast and efficient
//!
//! ## 2. `create_intelligent_clip()` - Intelligent Crop (TODO)
//! Handles: `Intelligent`
//! - Face detection and tracking
//! - Smart crop window computation
//! - Smooth camera motion
//! - Single view with face tracking
//!
//! ## 3. `create_intelligent_split_clip()` - Intelligent Split
//! Handles: `IntelligentSplit`
//! - Multi-step pipeline: extract halves → crop each → stack
//! - Future: Will integrate ML-based face tracking
//! - Currently uses placeholder scaling
//!
//! ## Style Routing Pattern
//!
//! **Caller Responsibility**: The processor must route to the correct function:
//! ```rust,ignore
//! match task.style {
//!     Style::Intelligent => create_intelligent_clip(...),
//!     Style::IntelligentSplit => create_intelligent_split_clip(...),
//!     _ => create_clip(...),
//! }
//! ```
//!
//! This matches the Python implementation's `run_ffmpeg_clip_with_crop()` logic.

use std::path::Path;
use tracing::info;

use vclip_models::{ClipTask, CropMode, EncodingConfig, Style};

use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::filters::build_video_filter;
use crate::progress::FfmpegProgress;
use crate::thumbnail::generate_thumbnail;

/// Extract a segment from a video file without re-encoding.
///
/// This is used to cut out a specific time range before applying
/// intelligent cropping, which significantly improves performance.
///
/// # Arguments
/// * `input` - Path to the input video file
/// * `output` - Path for the extracted segment
/// * `start_secs` - Start time in seconds
/// * `duration` - Duration in seconds
///
/// # Returns
/// Path to the extracted segment file
pub async fn extract_segment<P: AsRef<Path>>(
    input: P,
    output: P,
    start_secs: f64,
    duration: f64,
) -> MediaResult<()> {
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Extracting segment: {} -> {} (start: {:.2}s, duration: {:.2}s)",
        input.display(),
        output.display(),
        start_secs,
        duration
    );

    let cmd = FfmpegCommand::new(input, output)
        .seek(start_secs)
        .duration(duration)
        .codec_copy(); // Fast copy without re-encoding

    let runner = FfmpegRunner::new();
    runner.run(&cmd).await?;

    info!("Segment extracted: {}", output.display());
    Ok(())
}

/// Create a clip from a video file using traditional styles.
///
/// # Supported Styles
/// - `Original`: Preserves original aspect ratio
/// - `Split`: Left and right halves stacked (single-pass FFmpeg filter)
/// - `LeftFocus`: Left half expanded to portrait
/// - `RightFocus`: Right half expanded to portrait
///
/// # Not Supported
/// - `Intelligent`: Use `create_intelligent_clip()` instead (TODO)
/// - `IntelligentSplit`: Use `create_intelligent_split_clip()` instead
///
/// # Errors
/// Returns error if called with `Intelligent`, `IntelligentSplit` style or `Intelligent` crop mode.
pub async fn create_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Creating clip: {} -> {} (style: {}, crop: {})",
        input.display(),
        output.display(),
        task.style,
        task.crop_mode
    );

    // Parse timestamps
    let start_secs = parse_timestamp(&task.start)?;
    let end_secs = parse_timestamp(&task.end)?;

    // Apply padding
    let start_secs = (start_secs - task.pad_before).max(0.0);
    let end_secs = end_secs + task.pad_after;
    let duration = end_secs - start_secs;

    // Handle different styles and crop modes
    match (task.style, task.crop_mode) {
        // Original style always preserves original format
        (Style::Original, _) => {
            create_basic_clip(input, output, start_secs, duration, None, encoding, progress_callback).await?;
        }

        // Intelligent style uses face tracking - should be handled by caller
        (Style::Intelligent, _) => {
            return Err(MediaError::UnsupportedFormat(
                "Intelligent style must be processed using create_intelligent_clip - this is a caller error".to_string(),
            ));
        }

        // Intelligent split uses special processing - should be handled by caller
        (Style::IntelligentSplit, _) => {
            return Err(MediaError::UnsupportedFormat(
                "IntelligentSplit must be processed using create_intelligent_split_clip - this is a caller error".to_string(),
            ));
        }

        // Intelligent crop mode - not yet implemented
        (_, CropMode::Intelligent) => {
            return Err(MediaError::UnsupportedFormat(
                "Intelligent crop mode requires ML client integration (not yet implemented)".to_string(),
            ));
        }

        // Traditional styles (Split, LeftFocus, RightFocus)
        _ => {
            let filter = build_video_filter(task.style);
            create_basic_clip(input, output, start_secs, duration, filter.as_deref(), encoding, progress_callback).await?;
        }
    }

    // Generate thumbnail
    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        // Log but don't fail
        tracing::warn!("Failed to generate thumbnail: {}", e);
    }

    Ok(())
}

/// Create a basic clip with optional video filter.
async fn create_basic_clip<P, F>(
    input: P,
    output: P,
    start_secs: f64,
    duration: f64,
    filter: Option<&str>,
    encoding: &EncodingConfig,
    progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(FfmpegProgress) + Send + 'static,
{
    let mut cmd = FfmpegCommand::new(input, output)
        .seek(start_secs)
        .duration(duration)
        .video_codec(&encoding.codec)
        .preset(&encoding.preset)
        .crf(encoding.crf)
        .audio_codec(&encoding.audio_codec)
        .audio_bitrate(&encoding.audio_bitrate);

    if let Some(f) = filter {
        cmd = cmd.video_filter(f);
    }

    FfmpegRunner::new()
        .run_with_progress(&cmd, progress_callback)
        .await
}

/// Create an intelligent split clip (left and right halves with face tracking, stacked).
///
/// This is a multi-step process:
/// 1. Extract left and right halves
/// 2. Apply intelligent crop to each half (via ML client)
/// 3. Stack the results vertically
pub async fn create_intelligent_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Creating intelligent split clip: {} -> {}",
        input.display(),
        output.display()
    );

    // Parse timestamps
    let start_secs = parse_timestamp(&task.start)?;
    let end_secs = parse_timestamp(&task.end)?;
    let start_secs = (start_secs - task.pad_before).max(0.0);
    let end_secs = end_secs + task.pad_after;
    let duration = end_secs - start_secs;

    // Create temp directory for intermediate files
    let temp_dir = tempfile::tempdir()?;

    // Step 1: Extract left half
    let left_half = temp_dir.path().join("left.mp4");
    let cmd_left = FfmpegCommand::new(input, &left_half)
        .seek(start_secs)
        .duration(duration)
        .video_filter("crop=iw/2:ih:0:0")
        .video_codec(&encoding.codec)
        .preset(&encoding.preset)
        .crf(encoding.crf)
        .audio_codec("copy");

    FfmpegRunner::new().run(&cmd_left).await?;

    // Step 2: Extract right half
    let right_half = temp_dir.path().join("right.mp4");
    let cmd_right = FfmpegCommand::new(input, &right_half)
        .seek(start_secs)
        .duration(duration)
        .video_filter("crop=iw/2:ih:iw/2:0")
        .video_codec(&encoding.codec)
        .preset(&encoding.preset)
        .crf(encoding.crf)
        .audio_codec("copy");

    FfmpegRunner::new().run(&cmd_right).await?;

    // Step 3: Apply intelligent crop to each half (placeholder - needs ML client)
    // For now, just scale to target aspect
    let left_cropped = temp_dir.path().join("left_crop.mp4");
    let right_cropped = temp_dir.path().join("right_crop.mp4");

    // Scale each half to 9:8 aspect ratio (1080x960)
    let scale_filter = "scale=1080:960:force_original_aspect_ratio=decrease,pad=1080:960:(ow-iw)/2:(oh-ih)/2";

    let cmd_left_crop = FfmpegCommand::new(&left_half, &left_cropped)
        .video_filter(scale_filter)
        .video_codec(&encoding.codec)
        .preset(&encoding.preset)
        .crf(encoding.crf)
        .audio_codec("copy");

    FfmpegRunner::new().run(&cmd_left_crop).await?;

    let cmd_right_crop = FfmpegCommand::new(&right_half, &right_cropped)
        .video_filter(scale_filter)
        .video_codec(&encoding.codec)
        .preset(&encoding.preset)
        .crf(encoding.crf)
        .audio_codec("copy");

    FfmpegRunner::new().run(&cmd_right_crop).await?;

    // Step 4: Stack halves vertically
    // Increase CRF for final stacking (split views have high visual complexity)
    let final_crf = encoding.crf.saturating_add(4);

    // Build stacking command with two inputs
    let stack_args = vec![
        "-y".to_string(),
        "-i".to_string(), left_cropped.to_string_lossy().to_string(),
        "-i".to_string(), right_cropped.to_string_lossy().to_string(),
        "-filter_complex".to_string(), "[0:v][1:v]vstack".to_string(),
        "-c:v".to_string(), encoding.codec.clone(),
        "-preset".to_string(), encoding.preset.clone(),
        "-crf".to_string(), final_crf.to_string(),
        "-c:a".to_string(), encoding.audio_codec.clone(),
        "-b:a".to_string(), encoding.audio_bitrate.clone(),
        output.to_string_lossy().to_string(),
    ];

    let output_status = tokio::process::Command::new("ffmpeg")
        .args(&stack_args)
        .output()
        .await?;

    if !output_status.status.success() {
        return Err(MediaError::ffmpeg_failed(
            "Stacking failed",
            Some(String::from_utf8_lossy(&output_status.stderr).to_string()),
            output_status.status.code(),
        ));
    }

    // Generate thumbnail
    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        tracing::warn!("Failed to generate thumbnail: {}", e);
    }

    Ok(())
}

/// Parse timestamp string (HH:MM:SS or HH:MM:SS.mmm) to seconds.
fn parse_timestamp(ts: &str) -> MediaResult<f64> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return Err(MediaError::InvalidTimestamp(ts.to_string()));
    }

    let hours: f64 = parts[0]
        .parse()
        .map_err(|_| MediaError::InvalidTimestamp(ts.to_string()))?;
    let minutes: f64 = parts[1]
        .parse()
        .map_err(|_| MediaError::InvalidTimestamp(ts.to_string()))?;
    let seconds: f64 = parts[2]
        .parse()
        .map_err(|_| MediaError::InvalidTimestamp(ts.to_string()))?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp() {
        assert!((parse_timestamp("00:00:00").unwrap()).abs() < 0.001);
        assert!((parse_timestamp("00:01:00").unwrap() - 60.0).abs() < 0.001);
        assert!((parse_timestamp("01:00:00").unwrap() - 3600.0).abs() < 0.001);
        assert!((parse_timestamp("00:00:30.500").unwrap() - 30.5).abs() < 0.001);
    }
}
