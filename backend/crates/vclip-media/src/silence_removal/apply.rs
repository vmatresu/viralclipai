//! Apply silence removal using FFmpeg.
//!
//! This module takes the Keep/Cut segments from the segmenter and uses
//! FFmpeg to produce an output video with only the Keep segments.
//!
//! # FFmpeg Approaches
//!
//! 1. **Simple trim** (1 segment): Just use -ss and -t
//! 2. **Complex filter** (< 100 segments): Use filter_complex with trim/concat
//! 3. **Concat demuxer** (100+ segments): Write segment files and concat
//!
//! The approach is automatically selected based on segment count.

use std::path::Path;

use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, warn};

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

/// Apply silence removal to a video file.
///
/// This concatenates only the Keep segments using FFmpeg.
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
        "Applying silence removal"
    );

    // Choose approach based on segment count
    if keep_segments.len() == 1 {
        // Simple case: just trim
        apply_single_segment(input_path, output_path, keep_segments[0]).await
    } else if keep_segments.len() <= 100 {
        // Use filter_complex
        apply_filter_complex(input_path, output_path, &keep_segments).await
    } else {
        // Use concat demuxer for many segments
        apply_concat_demuxer(input_path, output_path, &keep_segments).await
    }
}

/// Apply silence removal when there's only one Keep segment.
async fn apply_single_segment(
    input_path: &Path,
    output_path: &Path,
    segment: &Segment,
) -> ApplyResult<()> {
    let start_sec = segment.start_ms as f64 / 1000.0;
    let duration_sec = segment.duration_secs();

    debug!(
        start_sec = start_sec,
        duration_sec = duration_sec,
        "Applying single segment trim"
    );

    let output = Command::new("ffmpeg")
        .args([
            "-ss",
            &format!("{:.3}", start_sec),
            "-i",
            input_path.to_str().unwrap_or_default(),
            "-t",
            &format!("{:.3}", duration_sec),
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "23",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-y",
            output_path.to_str().unwrap_or_default(),
        ])
        .output()
        .await
        .map_err(|e| ApplyError::FfmpegFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApplyError::FfmpegFailed(format!(
            "FFmpeg exited with code {:?}: {}",
            output.status.code(),
            stderr.lines().last().unwrap_or("Unknown error")
        )));
    }

    Ok(())
}

/// Apply silence removal using FFmpeg filter_complex.
///
/// This builds a complex filter that trims and concatenates segments.
async fn apply_filter_complex(
    input_path: &Path,
    output_path: &Path,
    segments: &[&Segment],
) -> ApplyResult<()> {
    let filter = build_concat_filter(segments);

    debug!(
        segments = segments.len(),
        filter_len = filter.len(),
        "Using filter_complex approach"
    );

    // Check if filter is too long for command line (typically 128KB limit on Linux)
    if filter.len() > 100_000 {
        warn!(
            filter_len = filter.len(),
            "Filter string very long, may fail on some systems"
        );
    }

    let output = Command::new("ffmpeg")
        .args([
            "-i",
            input_path.to_str().unwrap_or_default(),
            "-filter_complex",
            &filter,
            "-map",
            "[outv]",
            "-map",
            "[outa]",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "23",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-y",
            output_path.to_str().unwrap_or_default(),
        ])
        .output()
        .await
        .map_err(|e| ApplyError::FfmpegFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApplyError::FfmpegFailed(format!(
            "FFmpeg filter_complex failed: {}",
            stderr.lines().last().unwrap_or("Unknown error")
        )));
    }

    Ok(())
}

/// Build FFmpeg filter_complex string for concatenating segments.
fn build_concat_filter(segments: &[&Segment]) -> String {
    let mut filter = String::new();
    let mut v_labels = Vec::new();
    let mut a_labels = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        let start_sec = seg.start_ms as f64 / 1000.0;
        let end_sec = seg.end_ms as f64 / 1000.0;

        // Video trim: [0:v] -> trim -> setpts -> [vN]
        filter.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{}];",
            start_sec, end_sec, i
        ));
        v_labels.push(format!("[v{}]", i));

        // Audio trim: [0:a] -> atrim -> asetpts -> [aN]
        filter.push_str(&format!(
            "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{}];",
            start_sec, end_sec, i
        ));
        a_labels.push(format!("[a{}]", i));
    }

    // Concatenate all video segments
    let n = segments.len();
    filter.push_str(&format!(
        "{}concat=n={}:v=1:a=0[outv];",
        v_labels.join(""),
        n
    ));

    // Concatenate all audio segments
    filter.push_str(&format!(
        "{}concat=n={}:v=0:a=1[outa]",
        a_labels.join(""),
        n
    ));

    filter
}

/// Apply silence removal using FFmpeg concat demuxer.
///
/// This is used when there are too many segments for filter_complex.
/// It works by creating individual segment files and a concat list.
async fn apply_concat_demuxer(
    input_path: &Path,
    output_path: &Path,
    segments: &[&Segment],
) -> ApplyResult<()> {
    debug!(
        segments = segments.len(),
        "Using concat demuxer approach for many segments"
    );

    // Create temp directory for segment files
    let temp_dir = tempfile::tempdir()?;
    let mut segment_paths = Vec::new();

    // Extract each segment to a temp file
    for (i, seg) in segments.iter().enumerate() {
        let seg_path = temp_dir.path().join(format!("seg_{:04}.mp4", i));

        let start_sec = seg.start_ms as f64 / 1000.0;
        let duration_sec = seg.duration_secs();

        let output = Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.3}", start_sec),
                "-i",
                input_path.to_str().unwrap_or_default(),
                "-t",
                &format!("{:.3}", duration_sec),
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast", // Fast extraction
                "-crf",
                "18",        // Higher quality for intermediate
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-y",
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

    // Concatenate using concat demuxer
    let output = Command::new("ffmpeg")
        .args([
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            concat_list.to_str().unwrap_or_default(),
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "23",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-y",
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

    debug!(
        segments = segments.len(),
        "Concat demuxer approach completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_concat_filter_two_segments() {
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 1000,
                label: SegmentLabel::Keep,
            },
            Segment {
                start_ms: 2000,
                end_ms: 3000,
                label: SegmentLabel::Keep,
            },
        ];

        let refs: Vec<_> = segments.iter().collect();
        let filter = build_concat_filter(&refs);

        // Should have video and audio trim for each segment
        assert!(filter.contains("trim=start=0.000:end=1.000"));
        assert!(filter.contains("trim=start=2.000:end=3.000"));
        assert!(filter.contains("atrim=start=0.000:end=1.000"));
        assert!(filter.contains("atrim=start=2.000:end=3.000"));

        // Should have concat for both video and audio
        assert!(filter.contains("[v0][v1]concat=n=2:v=1:a=0[outv]"));
        assert!(filter.contains("[a0][a1]concat=n=2:v=0:a=1[outa]"));
    }

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
