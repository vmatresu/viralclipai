//! Left focus style processor.
//!
//! Handles video processing with left half focus - left half expanded
//! to portrait aspect ratio.

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
use super::utils;

/// Processor for left focus video style.
/// Expands the left half of the video to portrait aspect ratio.
#[derive(Clone)]
pub struct LeftFocusProcessor;

impl LeftFocusProcessor {
    /// Create a new left focus processor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LeftFocusProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for LeftFocusProcessor {
    fn name(&self) -> &'static str {
        "left_focus"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::LeftFocus)
    }

    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        ctx.security.check_resource_limits("ffmpeg")?;
        Ok(())
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        utils::run_basic_style(request, ctx, "left_focus").await
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
    async fn test_left_focus_processor_creation() {
        let processor = LeftFocusProcessor::new();
        assert_eq!(processor.name(), "left_focus");
        assert!(processor.can_handle(Style::LeftFocus));
        assert!(!processor.can_handle(Style::Split));
    }
}
