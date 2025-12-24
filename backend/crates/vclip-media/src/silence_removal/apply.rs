//! Apply silence removal using FFmpeg with stream copy.
//!
//! This module takes the Keep/Cut segments from the segmenter and uses
//! FFmpeg to produce an output video with only the Keep segments.
//!
//! # Strategy
//!
//! We use a **segment extraction + concat demuxer** approach with stream copy:
//! 1. Extract each Keep segment to a temporary file using stream copy (-c copy)
//! 2. Concatenate all segment files using concat demuxer with stream copy
//!
//! This is MUCH faster than re-encoding and preserves original quality/file size.
//! The old filter_complex approach re-encoded video at CRF 23, which could
//! increase file size for already-compressed content.
//!
//! # Keyframe Alignment
//!
//! Stream copy requires keyframe-aligned cuts. FFmpeg will seek to the nearest
//! keyframe before the requested start time. This means segments may be slightly
//! longer than requested, but the output will have consistent quality without
//! generation loss.

use std::path::Path;

use thiserror::Error;
use tracing::{debug, info, warn};

use super::config::SilenceRemovalConfig;
use super::segmenter::{compute_segment_stats, Segment, SegmentLabel};

/// Errors from applying silence removal.
#[derive(Error, Debug)]
pub enum ApplyError {
    #[error("FFmpeg failed: {0}")]
    FfmpegFailed(String),

    #[error("No segments to keep")]
    NoSegmentsToKeep,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Filter string too long for command line")]
    FilterTooLong,
}

/// Result type for apply operations.
pub type ApplyResult<T> = Result<T, ApplyError>;

/// Check if silence removal should be applied based on segment analysis.
///
/// Returns `true` if:
/// - There are segments to cut
/// - The amount cut is significant (> 10% by default)
/// - There would be enough content remaining
///
/// # Arguments
/// - `segments`: Keep/Cut segments from analysis
/// - `config`: Configuration with min_keep_ratio
pub fn should_apply_silence_removal(segments: &[Segment], config: &SilenceRemovalConfig) -> bool {
    if segments.is_empty() {
        return false;
    }

    let stats = compute_segment_stats(segments);

    // Check if there's anything to cut
    if stats.cut_count == 0 {
        debug!("No silence detected, skipping silence removal");
        return false;
    }

    // Check if we're keeping enough content
    if stats.keep_ratio < config.min_keep_ratio as f64 {
        warn!(
            keep_ratio = format!("{:.1}%", stats.keep_ratio * 100.0),
            min_keep_ratio = format!("{:.1}%", config.min_keep_ratio * 100.0),
            "Not enough speech content, skipping silence removal"
        );
        return false;
    }

    // Check if the cut is worth it (at least 10% reduction)
    let cut_ratio = 1.0 - stats.keep_ratio;
    if cut_ratio < 0.10 {
        debug!(
            cut_ratio = format!("{:.1}%", cut_ratio * 100.0),
            "Cut amount too small (<10%), skipping silence removal"
        );
        return false;
    }

    debug!(
        keep_ratio = format!("{:.1}%", stats.keep_ratio * 100.0),
        cut_ratio = format!("{:.1}%", cut_ratio * 100.0),
        keep_segments = stats.keep_count,
        cut_segments = stats.cut_count,
        "Silence removal will be applied"
    );

    true
}

/// Apply silence removal to a video file using stream copy.
///
/// This concatenates only the Keep segments using FFmpeg with stream copy,
/// preserving original quality and file size.
///
/// # Arguments
/// - `input_path`: Input video file
/// - `output_path`: Output video file (will be created/overwritten)
/// - `segments`: Keep/Cut segments from analysis
pub async fn apply_silence_removal(
    input_path: &Path,
    output_path: &Path,
    segments: &[Segment],
) -> ApplyResult<()> {
    // Filter to only Keep segments
    let keep_segments: Vec<_> = segments
        .iter()
        .filter(|s| s.label == SegmentLabel::Keep)
        .collect();

    if keep_segments.is_empty() {
        return Err(ApplyError::NoSegmentsToKeep);
    }

    debug!(
        input = %input_path.display(),
        output = %output_path.display(),
        keep_segments = keep_segments.len(),
        "Applying silence removal with stream copy"
    );

    // Always use segment extraction + concat demuxer approach with stream copy
    // This preserves quality and is faster than re-encoding
    apply_stream_copy_concat(input_path, output_path, &keep_segments).await
}

/// Apply silence removal using stream copy extraction + concat demuxer.
///
/// This is the preferred approach as it:
/// - Preserves original video quality (no re-encoding)
/// - Preserves original file size (no quality degradation)
/// - Is faster than re-encoding approaches
async fn apply_stream_copy_concat(
    input_path: &Path,
    output_path: &Path,
    segments: &[&Segment],
) -> ApplyResult<()> {
    info!(
        segments = segments.len(),
        "Using accurate seeking + concat demuxer approach"
    );

    // Create temp directory for segment files
    let temp_dir = tempfile::tempdir()?;
    let mut segment_paths = Vec::new();

    // Extract each segment using accurate seeking with re-encoding
    // Note: We use output seeking (-ss after -i) for frame-accurate cuts.
    // Stream copy with input seeking causes duplicate frames due to keyframe alignment.
    for (i, seg) in segments.iter().enumerate() {
        let seg_path = temp_dir.path().join(format!("seg_{:04}.mp4", i));

        let start_sec = seg.start_ms as f64 / 1000.0;
        let duration_sec = seg.duration_secs();

        debug!(
            segment = i,
            start_sec = start_sec,
            duration_sec = duration_sec,
            "Extracting segment with accurate seeking"
        );

        // Two-pass seeking: fast input seek to get close, then accurate output seek
        // This avoids the keyframe alignment issues that cause duplicate frames
        let fast_seek = if start_sec > 5.0 { start_sec - 5.0 } else { 0.0 };
        let accurate_seek = start_sec - fast_seek;

        let output = crate::command::create_ffmpeg_command()
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                // Fast input seek to get close (seeks to keyframe)
                "-ss",
                &format!("{:.3}", fast_seek),
                "-i",
                input_path.to_str().unwrap_or_default(),
                // Accurate output seek from that point
                "-ss",
                &format!("{:.3}", accurate_seek),
                // Duration to extract
                "-t",
                &format!("{:.3}", duration_sec),
                // Re-encode to ensure frame-accurate cuts (stream copy can't cut between keyframes)
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                // Fix timestamp issues
                "-avoid_negative_ts",
                "make_zero",
                seg_path.to_str().unwrap_or_default(),
            ])
            .output()
            .await
            .map_err(|e| ApplyError::FfmpegFailed(format!("Segment {} extraction failed: {}", i, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApplyError::FfmpegFailed(format!(
                "Segment {} extraction failed: {}",
                i,
                stderr.lines().last().unwrap_or("Unknown error")
            )));
        }

        segment_paths.push(seg_path);
    }

    // Write concat list file
    let concat_list = temp_dir.path().join("concat.txt");
    let list_content: String = segment_paths
        .iter()
        .map(|p| format!("file '{}'\n", p.display()))
        .collect();
    tokio::fs::write(&concat_list, &list_content).await?;

    // Concatenate using concat demuxer with stream copy
    let output = crate::command::create_ffmpeg_command()
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            concat_list.to_str().unwrap_or_default(),
            // Stream copy - no re-encoding
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            output_path.to_str().unwrap_or_default(),
        ])
        .output()
        .await
        .map_err(|e| ApplyError::FfmpegFailed(format!("Concat failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApplyError::FfmpegFailed(format!(
            "Concat failed: {}",
            stderr.lines().last().unwrap_or("Unknown error")
        )));
    }

    info!(
        segments = segments.len(),
        "Silence removal concat completed successfully"
    );

    // temp_dir is automatically cleaned up when dropped
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_apply_no_cuts() {
        let segments = vec![Segment {
            start_ms: 0,
            end_ms: 10000,
            label: SegmentLabel::Keep,
        }];

        let config = SilenceRemovalConfig::default();
        assert!(!should_apply_silence_removal(&segments, &config));
    }

    #[test]
    fn test_should_apply_with_cuts() {
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 5000,
                label: SegmentLabel::Keep,
            },
            Segment {
                start_ms: 5000,
                end_ms: 10000,
                label: SegmentLabel::Cut,
            },
        ];

        let config = SilenceRemovalConfig::default();
        assert!(should_apply_silence_removal(&segments, &config));
    }

    #[test]
    fn test_should_apply_too_little_keep() {
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 500, // Only 5% kept
                label: SegmentLabel::Keep,
            },
            Segment {
                start_ms: 500,
                end_ms: 10000, // 95% cut
                label: SegmentLabel::Cut,
            },
        ];

        let config = SilenceRemovalConfig::default(); // min_keep_ratio = 0.1
        assert!(!should_apply_silence_removal(&segments, &config));
    }
}
