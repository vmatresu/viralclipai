//! Audio analysis for silence detection.
//!
//! This module handles:
//! 1. Extracting audio from video files
//! 2. Converting to 16kHz mono f32 format
//! 3. Running VAD on audio frames
//! 4. Producing Keep/Cut segments

use std::path::Path;

use tempfile::NamedTempFile;
use thiserror::Error;
use tracing::debug;

use super::config::SilenceRemovalConfig;
use super::segmenter::{compute_segment_stats, Segment, SilenceRemover};
use super::vad::{SileroVad, VadError};

/// Errors from audio analysis.
#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("FFmpeg audio extraction failed: {0}")]
    AudioExtractionFailed(String),

    #[error("Failed to read audio file: {0}")]
    AudioReadFailed(#[from] std::io::Error),

    #[error("VAD error: {0}")]
    VadError(#[from] VadError),

    #[error("No audio data found in file")]
    NoAudioData,

    #[error("Audio too short for analysis")]
    AudioTooShort,
}

/// Result type for analysis operations.
pub type AnalysisResult<T> = Result<T, AnalysisError>;

/// Sample rate for VAD processing (Silero VAD v5 works best at 16kHz).
const VAD_SAMPLE_RATE: usize = 16000;

/// Analyze audio from a video/audio file and return Keep/Cut segments.
///
/// This is the main entry point for silence detection.
///
/// # Arguments
/// - `input_path`: Path to video or audio file
/// - `config`: Silence removal configuration
///
/// # Returns
/// Vector of `Segment` with Keep or Cut labels
pub async fn analyze_audio_segments(
    input_path: &Path,
    config: SilenceRemovalConfig,
) -> AnalysisResult<Vec<Segment>> {
    debug!(
        path = %input_path.display(),
        vad_threshold = config.vad_threshold,
        min_silence_ms = config.min_silence_ms,
        "Starting audio analysis for silence detection"
    );

    // Extract audio to temporary file
    let temp_audio = NamedTempFile::new()?;
    extract_audio_for_vad(input_path, temp_audio.path()).await?;

    // Load audio samples
    let samples = load_audio_samples(temp_audio.path()).await?;

    if samples.is_empty() {
        return Err(AnalysisError::NoAudioData);
    }

    // Calculate total duration
    let total_duration_ms = (samples.len() as u64 * 1000) / VAD_SAMPLE_RATE as u64;

    if total_duration_ms < config.min_silence_ms {
        debug!(
            duration_ms = total_duration_ms,
            min_silence_ms = config.min_silence_ms,
            "Audio too short for meaningful silence detection"
        );
        return Err(AnalysisError::AudioTooShort);
    }

    debug!(
        samples = samples.len(),
        duration_ms = total_duration_ms,
        "Loaded audio samples"
    );

    // Initialize VAD
    let mut vad = SileroVad::with_threshold(VAD_SAMPLE_RATE, config.vad_threshold)?;
    let frame_size = vad.frame_size();
    let frame_duration_ms = vad.frame_duration_ms();

    // Process through segmenter
    let mut remover = SilenceRemover::new(config.clone());

    for (i, chunk) in samples.chunks(frame_size).enumerate() {
        // Skip incomplete final frame
        if chunk.len() < frame_size {
            break;
        }

        let speech_prob = vad.analyze_frame(chunk)?;
        let timestamp_ms = (i as u64) * frame_duration_ms;
        remover.ingest_frame(speech_prob, timestamp_ms);
    }

    let segments = remover.finalize(total_duration_ms);

    // Log statistics
    let stats = compute_segment_stats(&segments);
    debug!(
        keep_ms = stats.total_keep_ms,
        cut_ms = stats.total_cut_ms,
        keep_ratio = format!("{:.1}%", stats.keep_ratio * 100.0),
        keep_segments = stats.keep_count,
        cut_segments = stats.cut_count,
        "Silence analysis complete"
    );

    Ok(segments)
}

/// Extract audio from a video file to 16kHz mono raw PCM.
///
/// Uses FFmpeg to convert any input format to the format expected by VAD.
async fn extract_audio_for_vad(input: &Path, output: &Path) -> AnalysisResult<()> {
    debug!(
        input = %input.display(),
        output = %output.display(),
        "Extracting audio for VAD"
    );

    let status = crate::command::create_ffmpeg_command()
        .args([
            "-i",
            input.to_str().unwrap_or_default(),
            "-vn", // No video
            "-ar",
            &VAD_SAMPLE_RATE.to_string(), // 16kHz
            "-ac",
            "1", // Mono
            "-f",
            "f32le", // Raw 32-bit float little-endian
            "-y",    // Overwrite
            output.to_str().unwrap_or_default(),
        ])
        // Suppress FFmpeg's verbose output
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| AnalysisError::AudioExtractionFailed(e.to_string()))?;

    if !status.success() {
        return Err(AnalysisError::AudioExtractionFailed(format!(
            "FFmpeg exited with code: {:?}",
            status.code()
        )));
    }

    // Verify output file exists and has content
    let metadata = tokio::fs::metadata(output).await?;
    if metadata.len() == 0 {
        return Err(AnalysisError::NoAudioData);
    }

    debug!(
        output_size = metadata.len(),
        "Audio extraction complete"
    );

    Ok(())
}

/// Load raw f32le audio samples from a file.
async fn load_audio_samples(path: &Path) -> AnalysisResult<Vec<f32>> {
    let bytes = tokio::fs::read(path).await?;

    // Convert bytes to f32 samples (4 bytes per sample, little-endian)
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    Ok(samples)
}

/// Debug helper: dump VAD output to a JSON file for analysis.
///
/// This is useful for tuning VAD parameters.
#[cfg(feature = "debug-vad")]
pub async fn dump_vad_debug(
    input_path: &Path,
    output_path: &Path,
    config: &SilenceRemovalConfig,
) -> AnalysisResult<()> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct VadFrame {
        timestamp_ms: u64,
        speech_prob: f32,
    }

    let temp_audio = NamedTempFile::new()?;
    extract_audio_for_vad(input_path, temp_audio.path()).await?;
    let samples = load_audio_samples(temp_audio.path()).await?;

    let mut vad = SileroVad::with_threshold(VAD_SAMPLE_RATE, config.vad_threshold)?;
    let frame_size = vad.frame_size();
    let frame_duration_ms = vad.frame_duration_ms();

    let mut frames = Vec::new();

    for (i, chunk) in samples.chunks(frame_size).enumerate() {
        if chunk.len() < frame_size {
            break;
        }

        let speech_prob = vad.analyze_frame(chunk)?;
        let timestamp_ms = (i as u64) * frame_duration_ms;

        frames.push(VadFrame {
            timestamp_ms,
            speech_prob,
        });
    }

    let json = serde_json::to_string_pretty(&frames)
        .map_err(|e| AnalysisError::AudioReadFailed(std::io::Error::other(e)))?;

    tokio::fs::write(output_path, json).await?;

    info!(
        frames = frames.len(),
        output = %output_path.display(),
        "VAD debug output written"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_samples_empty_file() {
        let temp = NamedTempFile::new().unwrap();
        let samples = load_audio_samples(temp.path()).await.unwrap();
        assert!(samples.is_empty());
    }

    #[tokio::test]
    async fn test_load_samples_with_data() {
        let temp = NamedTempFile::new().unwrap();

        // Write some test f32 samples
        let test_samples: Vec<f32> = vec![0.0, 0.5, 1.0, -1.0];
        let bytes: Vec<u8> = test_samples
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        tokio::fs::write(temp.path(), &bytes).await.unwrap();

        let loaded = load_audio_samples(temp.path()).await.unwrap();
        assert_eq!(loaded.len(), 4);
        assert!((loaded[0] - 0.0).abs() < 0.001);
        assert!((loaded[1] - 0.5).abs() < 0.001);
        assert!((loaded[2] - 1.0).abs() < 0.001);
        assert!((loaded[3] - (-1.0)).abs() < 0.001);
    }
}
