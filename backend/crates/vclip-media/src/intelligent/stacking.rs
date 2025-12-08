use std::path::Path;

use tokio::process::Command;

use crate::error::{MediaError, MediaResult};
use vclip_models::EncodingConfig;

/// Stack two pre-cropped halves into a single 1080x1920 portrait stream.
///
/// We explicitly map only the stacked video output and a single audio stream
/// to avoid ffmpeg's default behavior of muxing extra input streams (which
/// doubles file size). Audio is taken from the first input if present.
pub async fn stack_halves(
    left_half: &Path,
    right_half: &Path,
    output: &Path,
    encoding: &EncodingConfig,
) -> MediaResult<()> {
    let stack_crf = encoding.crf.saturating_add(2);
    let filter = "[0:v][1:v]vstack=inputs=2[vout]".to_string();

    let stack_args = vec![
        "-y".to_string(),
        "-i".to_string(),
        left_half.to_string_lossy().to_string(),
        "-i".to_string(),
        right_half.to_string_lossy().to_string(),
        "-filter_complex".to_string(),
        filter,
        // Explicit stream mapping: keep only the stacked video and first input audio (if any)
        "-map".to_string(),
        "[vout]".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-c:v".to_string(),
        encoding.codec.clone(),
        "-preset".to_string(),
        encoding.preset.clone(),
        "-crf".to_string(),
        stack_crf.to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        encoding.audio_bitrate.clone(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        "-shortest".to_string(),
        "-map_metadata".to_string(),
        "-1".to_string(),
        output.to_string_lossy().to_string(),
    ];

    let stack_status = Command::new("ffmpeg").args(&stack_args).output().await?;

    if !stack_status.status.success() {
        return Err(MediaError::ffmpeg_failed(
            "Stacking failed",
            Some(String::from_utf8_lossy(&stack_status.stderr).to_string()),
            stack_status.status.code(),
        ));
    }

    Ok(())
}

