//! Intelligent split style processor.
//!
//! Handles video processing with intelligent split view - combines face tracking
//! with split-screen layout for optimal viewing of dual subjects.

use async_trait::async_trait;

use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use crate::core::observability::ProcessingLogger;
use super::utils;

/// Processor for intelligent split video style.
/// Combines split-screen layout with face tracking for optimal dual-subject viewing.
#[derive(Clone)]
pub struct IntelligentSplitProcessor;

impl IntelligentSplitProcessor {
    /// Create a new intelligent split processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the estimated file size multiplier for intelligent split processing.
    #[allow(dead_code)]
    fn size_multiplier(&self) -> f64 {
        1.3 // 130% of original size (complex processing pipeline)
    }
}

impl Default for IntelligentSplitProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for IntelligentSplitProcessor {
    fn name(&self) -> &'static str {
        "intelligent_split"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::IntelligentSplit)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Additional validation for intelligent split processing
        ctx.security.check_resource_limits("face_detection")?;
        ctx.security.check_resource_limits("ffmpeg")?;

        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        let timer = ctx.metrics.start_timer("intelligent_split_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            "intelligent_split".to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        // For now, delegate to the existing intelligent split implementation
        // In the full implementation, this would be refactored to use the new architecture
        let _result = crate::intelligent::create_intelligent_split_clip(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            &request.encoding,
            |_progress| {
                // Could emit progress updates
            },
        ).await?;

        let processing_time = timer.elapsed();

        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration = super::super::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0) -
                      super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(utils::thumbnail_path(&request.output_path).into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics.increment_counter("processing_completed", &[("style", "intelligent_split")]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", "intelligent_split")]
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> crate::core::ProcessingComplexity {
        let duration = super::super::intelligent::parse_timestamp(&request.task.end).unwrap_or(30.0) -
                      super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);
        utils::estimate_complexity(duration, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_intelligent_split_processor_creation() {
        let processor = IntelligentSplitProcessor::new();
        assert_eq!(processor.name(), "intelligent_split");
        assert!(processor.can_handle(Style::IntelligentSplit));
        assert!(!processor.can_handle(Style::Split));
    }
}
