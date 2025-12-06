//! Right focus style processor.
//!
//! Handles video processing with right half focus - right half expanded
//! to portrait aspect ratio.

use async_trait::async_trait;
use std::time::Instant;
use std::process::Stdio;

use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use crate::core::observability::ProcessingLogger;
use crate::filters;
use super::utils;

/// Processor for right focus video style.
/// Expands the right half of the video to portrait aspect ratio.
#[derive(Clone)]
pub struct RightFocusProcessor;

impl RightFocusProcessor {
    /// Create a new right focus processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the FFmpeg filter for right focus processing.
    fn get_filter(&self) -> &'static str {
        filters::FILTER_RIGHT_FOCUS
    }

    /// Get the estimated file size multiplier for right focus processing.
    #[allow(dead_code)]
    fn size_multiplier(&self) -> f64 {
        1.1 // 110% of original size (scaling operations)
    }
}

impl Default for RightFocusProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for RightFocusProcessor {
    fn name(&self) -> &'static str {
        "right_focus"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::RightFocus)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        ctx.security.check_resource_limits("ffmpeg")?;
        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        let timer = ctx.metrics.start_timer("right_focus_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            "right_focus".to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        let start_time = Instant::now();
        let start_secs = super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);
        let end_secs = super::super::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0);
        let duration = end_secs - start_secs;

        let filter = self.get_filter();

        let mut ffmpeg_args = vec![
            "-y".to_string(),
            "-ss".to_string(),
            format!("{:.3}", start_secs),
            "-i".to_string(),
            request.input_path.to_string_lossy().to_string(),
            "-t".to_string(),
            format!("{:.3}", duration),
            "-vf".to_string(),
            filter.to_string(),
            "-c:v".to_string(),
            request.encoding.codec.clone(),
            "-preset".to_string(),
            request.encoding.preset.clone(),
            "-crf".to_string(),
            request.encoding.crf.to_string(),
            "-c:a".to_string(),
            request.encoding.audio_codec.clone(),
            "-b:a".to_string(),
            request.encoding.audio_bitrate.clone(),
            request.output_path.to_string_lossy().to_string(),
        ];

        ffmpeg_args = ctx.security.sanitize_command(&ffmpeg_args)?;

        logger.log_progress("Executing FFmpeg", 25);

        let ffmpeg_result = tokio::process::Command::new("ffmpeg")
            .args(&ffmpeg_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match ffmpeg_result {
            Ok(output) if output.status.success() => {
                logger.log_progress("FFmpeg completed", 75);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                ctx.metrics.increment_counter("ffmpeg_error", &[("style", "right_focus")]);
                return Err(crate::error::MediaError::ffmpeg_failed(
                    "Right focus processing failed",
                    Some(stderr.to_string()),
                    output.status.code(),
                ));
            }
            Err(e) => {
                ctx.metrics.increment_counter("ffmpeg_error", &[("style", "right_focus")]);
                return Err(crate::error::MediaError::Io(e));
            }
        }

        logger.log_progress("Generating thumbnail", 90);
        let thumbnail_path = utils::thumbnail_path(&request.output_path);
        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let processing_time = start_time.elapsed();

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(thumbnail_path.into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics.increment_counter("processing_completed", &[("style", "right_focus")]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", "right_focus")]
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> crate::core::ProcessingComplexity {
        let duration = super::super::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0) -
                      super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);
        utils::estimate_complexity(duration, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_right_focus_processor_creation() {
        let processor = RightFocusProcessor::new();
        assert_eq!(processor.name(), "right_focus");
        assert!(processor.can_handle(Style::RightFocus));
        assert!(!processor.can_handle(Style::Split));
    }
}
