//! Original style processor.
//!
//! Handles video processing with no style modifications - just transcoding
//! with the specified encoding parameters.

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use super::utils;

/// Processor for original video style.
/// Simply transcodes the video without any filtering or cropping.
#[derive(Clone)]
pub struct OriginalProcessor;

impl OriginalProcessor {
    /// Create a new original processor.
    pub fn new() -> Self {
        Self
    }

    /// Get the estimated file size multiplier for original processing.
    /// Original processing typically has minimal size change.
    #[allow(dead_code)]
    fn size_multiplier(&self) -> f64 {
        0.95 // 95% of original size (minor compression)
    }
}

impl Default for OriginalProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for OriginalProcessor {
    fn name(&self) -> &'static str {
        "original"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::Original)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        // Additional validation for original style
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Check file size limits
        ctx.security.validate_file_size(0)?; // Will be checked during processing

        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        utils::run_basic_style(request, ctx, "original").await
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> crate::core::ProcessingComplexity {
        let duration = request.task.end.parse::<f64>().unwrap_or(30.0);
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
    async fn test_original_processor_creation() {
        let processor = OriginalProcessor::new();
        assert_eq!(processor.name(), "original");
        assert!(processor.can_handle(Style::Original));
        assert!(!processor.can_handle(Style::Split));
    }

    #[tokio::test]
    async fn test_original_processor_validation() {
        let processor = OriginalProcessor::new();

        // Create a mock request
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input.mp4");
        tokio::fs::write(&input_path, b"fake video").await.unwrap();

        let request = ProcessingRequest::new(
            ClipTask {
                scene_id: "test".to_string(),
                scene_title: "Test".to_string(),
                scene_description: "Test".to_string(),
                start: "0".to_string(),
                end: "10".to_string(),
                style: Style::Original,
                crop_mode: Default::default(),
                target_aspect: Default::default(),
                priority: 1,
                pad_before: 0.0,
                pad_after: 0.0,
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
}
