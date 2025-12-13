//! Video clipping operations.
//!
//! # Architecture
//!
//! This module provides the main entry point for traditional clip creation:
//!
//! ## `create_clip()` - Traditional Styles
//! Handles: `Original`, `Split`, `LeftFocus`, `CenterFocus`, `RightFocus`
//! - Uses FFmpeg video filters for transformations
//! - Single-pass processing
//! - Fast and efficient
//!
//! ## Intelligent Styles (see `intelligent` module)
//! - `create_intelligent_clip()` - Face tracking on full frame
//! - `create_tier_aware_split_clip_with_cache()` - Face detection with smart layout
//!
//! ## Style Routing Pattern
//!
//! **Note**: Style routing is now handled by the `StyleProcessorFactory` in `styles/mod.rs`.
//! Direct function calls are deprecated in favor of the processor pattern.

use std::path::Path;
use tracing::info;

use vclip_models::{ClipTask, CropMode, EncodingConfig, Style};

use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::filters::build_video_filter;
use crate::progress::FfmpegProgress;
use crate::thumbnail::generate_thumbnail;

/// Extract a segment from a video file using STREAM COPY (no re-encoding).
///
/// This is used to cut out a specific time range before applying
/// intelligent cropping. Uses stream copy for:
/// - **Speed**: No encoding overhead
/// - **Quality**: No generation loss
/// - **Size**: Same bitrate as source
///
/// # Seeking Strategy
/// Uses input seeking with keyframe alignment. The output may start
/// slightly before the requested time (at nearest keyframe), but this
/// is acceptable since we're feeding into another processing step.
///
/// # Arguments
/// * `input` - Path to the input video file (can be 1-2h source)
/// * `output` - Path for the extracted segment (~30s-1min)
/// * `start_secs` - Start time in seconds
/// * `duration` - Duration in seconds
pub async fn extract_segment<P: AsRef<Path>>(
    input: P,
    output: P,
    start_secs: f64,
    duration: f64,
) -> MediaResult<()> {
    let input = input.as_ref();
    let output = output.as_ref();

    let start_time = std::time::Instant::now();

    info!(
        "[SEGMENT_EXTRACT] START: {} -> {}",
        input.display(),
        output.display()
    );
    info!(
        "[SEGMENT_EXTRACT] Time range: {:.2}s to {:.2}s (duration: {:.2}s)",
        start_secs,
        start_secs + duration,
        duration
    );

    // Use stream copy - NO re-encoding!
    // -ss before -i: input seeking (fast, seeks to keyframe)
    // -c copy: stream copy (no decode/encode)
    // -avoid_negative_ts make_zero: fix any timestamp issues
    let args = vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        // Input seeking - fast, approximate to keyframe
        "-ss".to_string(), format!("{:.3}", start_secs),
        "-i".to_string(), input.to_string_lossy().to_string(),
        // Duration
        "-t".to_string(), format!("{:.3}", duration),
        // STREAM COPY - no re-encoding!
        "-c".to_string(), "copy".to_string(),
        // Fix timestamp issues that can occur with stream copy
        "-avoid_negative_ts".to_string(), "make_zero".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        output.to_string_lossy().to_string(),
    ];

    let status = tokio::process::Command::new("ffmpeg")
        .args(&args)
        .output()
        .await?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(crate::error::MediaError::ffmpeg_failed(
            "Segment extraction failed",
            Some(stderr.to_string()),
            status.status.code(),
        ));
    }

    let elapsed = start_time.elapsed();
    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!(
        "[SEGMENT_EXTRACT] DONE in {:.2}s - output: {} ({:.2} MB)",
        elapsed.as_secs_f64(),
        output.display(),
        file_size as f64 / 1_000_000.0
    );

    Ok(())
}

/// Create a clip from a video file using traditional styles.
///
/// # Supported Styles
/// - `Original`: Preserves original aspect ratio
/// - `Split`: Left and right halves stacked (single-pass FFmpeg filter)
/// - `LeftFocus`: Left half expanded to portrait
/// - `CenterFocus`: Center vertical slice expanded to portrait
/// - `RightFocus`: Right half expanded to portrait
///
/// # Not Supported
/// - `Intelligent`: Use `create_intelligent_clip()` instead
/// - `IntelligentSplit`: Use `create_tier_aware_split_clip_with_cache()` instead
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
                "IntelligentSplit must be processed using create_tier_aware_split_clip_with_cache - this is a caller error".to_string(),
            ));
        }

        // Intelligent crop mode - not yet implemented
        (_, CropMode::Intelligent) => {
            return Err(MediaError::UnsupportedFormat(
                "Intelligent crop mode requires ML client integration (not yet implemented)".to_string(),
            ));
        }

        // Traditional styles (Split, LeftFocus, CenterFocus, RightFocus)
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
