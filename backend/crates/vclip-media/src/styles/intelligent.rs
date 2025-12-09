//! Intelligent style processor.
//!
//! Handles video processing with intelligent face tracking and cropping.
//! Uses face detection to follow subjects and maintain optimal framing.
//!
//! Detection behavior is controlled by the `DetectionTier`:
//! - `Basic`: YuNet face detection only - follows most prominent face
//! - `SpeakerAware`: YuNet + audio + face activity - robust speaker tracking with hysteresis
//! - `MotionAware`: Visual motion + face detection - favors active movers
//! - `ActivityAware`: Full visual activity + face detection - high quality motion tracking
//!
//! # Tier Differences
//!
//! - **Basic**: Camera follows the largest/most confident face. Good for single-speaker content.
//! - **SpeakerAware**: Full activity tracking with hysteresis. Minimum dwell time (1.0s)
//!   before switching, requires 20% improvement margin. Best for multi-speaker podcasts.
//! - **MotionAware**: Uses visual motion to follow active faces. Works with mono audio.
//! - **ActivityAware**: Combines motion + size change analysis for stable tracking.

use async_trait::async_trait;
use tracing::info;
use vclip_models::{DetectionTier, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;

use super::utils;

/// Processor for intelligent video style.
/// Uses face detection and tracking for optimal cropping.
#[derive(Clone)]
pub struct IntelligentProcessor {
    /// Detection tier controlling which providers are used.
    tier: DetectionTier,
}

impl IntelligentProcessor {
    /// Create a new intelligent processor with Basic tier (default).
    pub fn new() -> Self {
        Self {
            tier: DetectionTier::Basic,
        }
    }

    /// Create an intelligent processor with a specific detection tier.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self { tier }
    }

    /// Get the detection tier.
    pub fn detection_tier(&self) -> DetectionTier {
        self.tier
    }

    /// Get the estimated file size multiplier for intelligent processing.
    /// Intelligent processing may have variable output sizes based on face detection.
    #[allow(dead_code)]
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
        match self.tier {
            DetectionTier::None => "intelligent_heuristic",
            DetectionTier::Basic => "intelligent",
            DetectionTier::SpeakerAware => "intelligent_speaker",
            DetectionTier::MotionAware => "intelligent_motion",
            DetectionTier::ActivityAware => "intelligent_activity",
        }
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(
            style,
            Style::Intelligent
                | Style::IntelligentBasic
                | Style::IntelligentSpeaker
                | Style::IntelligentMotion
                | Style::IntelligentActivity
        )
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Additional validation for intelligent processing
        ctx.security.check_resource_limits("face_detection")?;

        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        let tier_name = self.name();
        let timer = ctx.metrics.start_timer("intelligent_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            tier_name.to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        info!(
            "Processing with {} tier (detection: {:?})",
            tier_name, self.tier
        );

        // Use tier-aware intelligent cropper for tier-specific behavior
        // - Basic: Follows most prominent face (largest × confidence)
        // - SpeakerAware: Full activity tracking with hysteresis
        let _result = crate::intelligent::create_tier_aware_intelligent_clip(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            self.tier,
            &request.encoding,
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

        // Higher tiers require more processing time
        let multiplier = match self.tier {
            DetectionTier::None => 0.5,
            DetectionTier::Basic => 1.0,
            DetectionTier::MotionAware => 1.3,
            DetectionTier::SpeakerAware => 1.6,
            DetectionTier::ActivityAware => 1.7,
        };

        let mut complexity = utils::estimate_complexity(duration, true);
        complexity.estimated_time_ms = (complexity.estimated_time_ms as f64 * multiplier) as u64;
        complexity
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
        assert_eq!(processor.detection_tier(), DetectionTier::Basic);
    }

    #[tokio::test]
    async fn test_intelligent_processor_with_tier() {
        let processor = IntelligentProcessor::with_tier(DetectionTier::SpeakerAware);
        assert_eq!(processor.name(), "intelligent_speaker");
        assert_eq!(processor.detection_tier(), DetectionTier::SpeakerAware);

        let processor = IntelligentProcessor::with_tier(DetectionTier::MotionAware);
        assert_eq!(processor.name(), "intelligent_motion");
        assert_eq!(processor.detection_tier(), DetectionTier::MotionAware);
    }

    #[test]
    fn test_can_handle_all_intelligent_styles() {
        let processor = IntelligentProcessor::new();
        assert!(processor.can_handle(Style::Intelligent));
        assert!(processor.can_handle(Style::IntelligentBasic));
        assert!(processor.can_handle(Style::IntelligentSpeaker));

        assert!(!processor.can_handle(Style::IntelligentSplit));
        assert!(!processor.can_handle(Style::Split));
    }
}

