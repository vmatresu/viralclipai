//! Intelligent cinematic style processor.
//!
//! Handles video processing with AutoAI-inspired smooth camera motion.
//! Uses polynomial trajectory optimization for professional camera paths.
//!
//! # Features
//!
//! - **Smooth Camera Motion**: Polynomial curve fitting eliminates jitter
//! - **Camera Mode Selection**: Automatic stationary/panning/tracking selection
//! - **Adaptive Zoom**: Dynamic zoom based on subject count and activity

use async_trait::async_trait;
use tracing::info;
use vclip_models::{DetectionTier, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;
use crate::intelligent::cinematic;

use super::utils;

/// Processor for intelligent cinematic video style.
/// Uses polynomial trajectory optimization for smooth camera motion.
#[derive(Clone)]
pub struct IntelligentCinematicProcessor {
    /// Detection tier (always Cinematic for this processor).
    tier: DetectionTier,
}

impl IntelligentCinematicProcessor {
    /// Create a new cinematic processor.
    pub fn new() -> Self {
        Self {
            tier: DetectionTier::Cinematic,
        }
    }

    /// Get the detection tier.
    pub fn detection_tier(&self) -> DetectionTier {
        self.tier
    }
}

impl Default for IntelligentCinematicProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for IntelligentCinematicProcessor {
    fn name(&self) -> &'static str {
        "intelligent_cinematic"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::IntelligentCinematic)
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Additional validation for cinematic processing
        ctx.security.check_resource_limits("face_detection")?;

        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        let tier_name = self.name();
        let timer = ctx.metrics.start_timer("cinematic_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            tier_name.to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        let has_cache = request.has_cached_analysis();
        info!(
            "Processing with {} tier (detection: {:?}, cached: {})",
            tier_name, self.tier, has_cache
        );

        // Use cinematic processor with optional cached neural analysis
        cinematic::create_cinematic_clip_with_cache(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            &request.encoding,
            request.watermark.as_ref(),
            request.cached_neural_analysis.as_deref(),
            |_progress| {
                // Could emit progress updates
            },
        )
        .await?;

        let processing_time = timer.elapsed();

        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration = super::super::intelligent::parse_timestamp(&request.task.end)
            .unwrap_or(30.0)
            - super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(utils::thumbnail_path(&request.output_path).into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics
            .increment_counter("processing_completed", &[("style", tier_name)]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", tier_name)],
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> crate::core::ProcessingComplexity {
        let duration = super::super::intelligent::parse_timestamp(&request.task.end)
            .unwrap_or(30.0)
            - super::super::intelligent::parse_timestamp(&request.task.start).unwrap_or(0.0);

        // Cinematic processing has higher complexity due to trajectory optimization
        let multiplier = 1.8;

        let mut complexity = utils::estimate_complexity(duration, true);
        complexity.estimated_time_ms = (complexity.estimated_time_ms as f64 * multiplier) as u64;
        complexity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cinematic_processor_creation() {
        let processor = IntelligentCinematicProcessor::new();
        assert_eq!(processor.name(), "intelligent_cinematic");
        assert!(processor.can_handle(Style::IntelligentCinematic));
        assert!(!processor.can_handle(Style::Intelligent));
        assert!(!processor.can_handle(Style::Split));
        assert_eq!(processor.detection_tier(), DetectionTier::Cinematic);
    }

    #[test]
    fn test_can_handle_only_cinematic() {
        let processor = IntelligentCinematicProcessor::new();
        // Only cinematic style should be handled
        assert!(processor.can_handle(Style::IntelligentCinematic));

        // Other styles should NOT be handled
        assert!(!processor.can_handle(Style::Intelligent));
        assert!(!processor.can_handle(Style::IntelligentSpeaker));
        assert!(!processor.can_handle(Style::IntelligentSplit));
        assert!(!processor.can_handle(Style::Split));
    }
}
