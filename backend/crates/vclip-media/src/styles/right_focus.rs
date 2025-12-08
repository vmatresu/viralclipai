//! Right focus style processor.
//!
//! Handles video processing with right half focus - right half expanded
//! to portrait aspect ratio.

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor};
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
        utils::run_basic_style(request, ctx, "right_focus").await
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
