//! Video clipping operations.

use std::path::Path;
use tracing::info;

use vclip_models::{ClipTask, CropMode, EncodingConfig, Style};

use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::filters::build_video_filter;
use crate::progress::FfmpegProgress;
use crate::thumbnail::generate_thumbnail;

/// Create a clip from a video file.
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

        // Intelligent split uses special processing
        (Style::IntelligentSplit, _) => {
            // This is handled by create_intelligent_split_clip
            return Err(MediaError::UnsupportedFormat(
                "IntelligentSplit should use create_intelligent_split_clip".to_string(),
            ));
        }

        // Intelligent crop mode
        (_, CropMode::Intelligent) => {
            // This is handled by ML client
            return Err(MediaError::UnsupportedFormat(
                "Intelligent crop mode requires ML client".to_string(),
            ));
        }

        // Traditional styles
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
