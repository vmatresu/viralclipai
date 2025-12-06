//! Video clipping operations.
//!
//! # Architecture
//!
//! This module provides the main entry point for traditional clip creation:
//!
//! ## `create_clip()` - Traditional Styles
//! Handles: `Original`, `Split`, `LeftFocus`, `RightFocus`
//! - Uses FFmpeg video filters for transformations
//! - Single-pass processing
//! - Fast and efficient
//!
//! ## Intelligent Styles (see `intelligent` module)
//! - `create_intelligent_clip()` - Face tracking on full frame
//! - `create_intelligent_split_clip()` - Face detection with smart layout
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
