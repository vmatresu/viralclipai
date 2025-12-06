//! Intelligent style processor.
//!
//! Handles video processing with intelligent face tracking and cropping.
//! Uses face detection to follow subjects and maintain optimal framing.

use async_trait::async_trait;

use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use crate::core::observability::ProcessingLogger;
use super::utils;

/// Processor for intelligent video style.
/// Uses face detection and tracking for optimal cropping.
#[derive(Clone)]
pub struct IntelligentProcessor;

impl IntelligentProcessor {
    /// Create a new intelligent processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the estimated file size multiplier for intelligent processing.
    /// Intelligent processing may have variable output sizes based on face detection.
    fn size_multiplier(&self) -> f64 {
        1.0 // Variable based on detected content
    }
}

impl Default for IntelligentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for IntelligentProcessor {
    fn name(&self) -> &'static str {
        "intelligent"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::Intelligent)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Additional validation for intelligent processing
        ctx.security.check_resource_limits("face_detection")?;

        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        let timer = ctx.metrics.start_timer("intelligent_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            "intelligent".to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        // For now, delegate to the existing intelligent implementation
        // In the full implementation, this would be refactored to use the new architecture
        let _result = crate::intelligent::create_intelligent_clip(
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

        ctx.metrics.increment_counter("processing_completed", &[("style", "intelligent")]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", "intelligent")]
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
    async fn test_intelligent_processor_creation() {
        let processor = IntelligentProcessor::new();
        assert_eq!(processor.name(), "intelligent");
        assert!(processor.can_handle(Style::Intelligent));
        assert!(!processor.can_handle(Style::Split));
    }
}
