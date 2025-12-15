//! Split style processor.
//!
//! Handles video processing with split view - left and right halves
//! stacked vertically using FFmpeg filters.

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use super::utils;

/// Processor for split video style.
/// Uses FFmpeg filters to create a split-screen view.
#[derive(Clone)]
pub struct SplitProcessor;

impl SplitProcessor {
    /// Create a new split processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the FFmpeg filter string for split processing.
    pub fn get_filter(&self) -> &str {
        crate::filters::FILTER_SPLIT
    }
}

impl Default for SplitProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for SplitProcessor {
    fn name(&self) -> &'static str {
        "split"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::Split)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        // Additional validation for split style
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Validate that FFmpeg supports the required filters
        ctx.security.check_resource_limits("ffmpeg")?;

        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        utils::run_basic_style(request, ctx, "split").await
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
    use std::sync::Arc;
    use tokio::sync::Semaphore;
    use vclip_models::{ClipTask, EncodingConfig};

    #[tokio::test]
    async fn test_split_processor_creation() {
        let processor = SplitProcessor::new();
        assert_eq!(processor.name(), "split");
        assert!(processor.can_handle(Style::Split));
        assert!(!processor.can_handle(Style::Original));
    }

    #[tokio::test]
    async fn test_split_processor_validation() {
        let processor = SplitProcessor::new();

        // Create a mock request
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input.mp4");
        tokio::fs::write(&input_path, b"fake video").await.unwrap();

        let request = ProcessingRequest::new(
            ClipTask {
                scene_id: 1,
                scene_title: "Test".to_string(),
                scene_description: Some("Test".to_string()),
                start: "0".to_string(),
                end: "10".to_string(),
                style: Style::Split,
                crop_mode: Default::default(),
                target_aspect: Default::default(),
                priority: 1,
                pad_before: 0.0,
                pad_after: 0.0,
                streamer_split_params: None,
            },
            input_path,
            temp_dir.path().join("output.mp4"),
            EncodingConfig::default(),
            "test-request".to_string(),
            "test-user".to_string(),
        ).unwrap();

        let ctx = ProcessingContext::new(
            "test-request".to_string(),
            "test-user".to_string(),
            temp_dir.path(),
            Arc::new(Semaphore::new(1)),
            Arc::new(crate::core::observability::MetricsCollector::new()),
            Arc::new(crate::core::security::SecurityContext::new()),
        );

        // Should validate successfully
        assert!(processor.validate(&request, &ctx).await.is_ok());
    }

    #[test]
    fn test_filter_availability() {
        let processor = SplitProcessor::new();
        let filter = processor.get_filter();
        assert!(filter.contains("vstack"));
        assert!(filter.contains("crop"));
    }
}
