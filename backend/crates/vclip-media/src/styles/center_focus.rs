//! Center focus style processor.
//!
//! Crops the centered vertical slice of a landscape video to portrait aspect ratio.

use async_trait::async_trait;
use vclip_models::Style;

use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;

use super::utils;

/// Processor for center focus video style.
#[derive(Clone)]
pub struct CenterFocusProcessor;

impl CenterFocusProcessor {
    /// Create a new center focus processor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for CenterFocusProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for CenterFocusProcessor {
    fn name(&self) -> &'static str {
        "center_focus"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::CenterFocus)
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        ctx.security.check_resource_limits("ffmpeg")?;
        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        utils::run_basic_style(request, ctx, "center_focus").await
    }

    fn estimate_complexity(
        &self,
        request: &ProcessingRequest,
    ) -> crate::core::ProcessingComplexity {
        let duration = super::super::intelligent::parse_timestamp(&request.task.end)
            .unwrap_or(30.0)
            - super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);
        utils::estimate_complexity(duration, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_center_focus_processor_creation() {
        let processor = CenterFocusProcessor::new();
        assert_eq!(processor.name(), "center_focus");
        assert!(processor.can_handle(Style::CenterFocus));
        assert!(!processor.can_handle(Style::Split));
    }
}
