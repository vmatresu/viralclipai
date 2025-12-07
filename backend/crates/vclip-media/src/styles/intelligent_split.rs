//! Intelligent split style processor.
//!
//! Handles video processing with intelligent split view - combines face tracking
//! with split-screen layout for optimal viewing of dual subjects.
//!
//! Detection behavior is controlled by the `DetectionTier`:
//! - `Basic`: Fixed vertical positioning (fast, deterministic)
//! - `AudioAware`: Face-aware positioning with speaker detection
//! - `SpeakerAware`: Dynamic per-panel positioning based on face detection
//!
//! # Tier Differences for Split View
//!
//! - **Basic**: Uses fixed vertical positioning (0% for left, 15% for right).
//!   Fast and deterministic, good for consistent podcast layouts.
//! - **AudioAware**: Detects faces in each panel and adjusts vertical positioning
//!   to ensure faces are fully visible. Uses speaker detection for logging.
//! - **SpeakerAware**: Full face detection with dynamic positioning. Analyzes
//!   face positions in each panel and computes optimal vertical offset.

use async_trait::async_trait;
use tracing::info;
use vclip_models::{DetectionTier, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;

use super::utils;

/// Processor for intelligent split video style.
/// Combines split-screen layout with face tracking for optimal dual-subject viewing.
#[derive(Clone)]
pub struct IntelligentSplitProcessor {
    /// Detection tier controlling which providers are used.
    tier: DetectionTier,
}

impl IntelligentSplitProcessor {
    /// Create a new intelligent split processor with Basic tier (default).
    pub fn new() -> Self {
        Self {
            tier: DetectionTier::Basic,
        }
    }

    /// Create an intelligent split processor with a specific detection tier.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self { tier }
    }

    /// Get the detection tier.
    pub fn detection_tier(&self) -> DetectionTier {
        self.tier
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
        match self.tier {
            DetectionTier::None => "intelligent_split_heuristic",
            DetectionTier::Basic => "intelligent_split",
            DetectionTier::AudioAware => "intelligent_split_audio",
            DetectionTier::SpeakerAware => "intelligent_split_speaker",
        }
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(
            style,
            Style::IntelligentSplit
                | Style::IntelligentSplitBasic
                | Style::IntelligentSplitAudio
                | Style::IntelligentSplitSpeaker
        )
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;

        // Additional validation for intelligent split processing
        ctx.security.check_resource_limits("face_detection")?;
        ctx.security.check_resource_limits("ffmpeg")?;

        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        let tier_name = self.name();
        let timer = ctx.metrics.start_timer("intelligent_split_processing");
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

        // Use tier-aware split processor for tier-specific behavior
        // - Basic: Fixed vertical positioning (0% left, 15% right)
        // - AudioAware/SpeakerAware: Face-aware positioning per panel
        let _result = crate::intelligent::create_tier_aware_split_clip(
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

        // Higher tiers require more processing time; split adds overhead
        let multiplier = match self.tier {
            DetectionTier::None => 0.6,
            DetectionTier::Basic => 1.2,
            DetectionTier::AudioAware => 1.5,
            DetectionTier::SpeakerAware => 1.8,
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
    async fn test_intelligent_split_processor_creation() {
        let processor = IntelligentSplitProcessor::new();
        assert_eq!(processor.name(), "intelligent_split");
        assert!(processor.can_handle(Style::IntelligentSplit));
        assert!(!processor.can_handle(Style::Split));
        assert_eq!(processor.detection_tier(), DetectionTier::Basic);
    }

    #[tokio::test]
    async fn test_intelligent_split_processor_with_tier() {
        let processor = IntelligentSplitProcessor::with_tier(DetectionTier::AudioAware);
        assert_eq!(processor.name(), "intelligent_split_audio");
        assert_eq!(processor.detection_tier(), DetectionTier::AudioAware);

        let processor = IntelligentSplitProcessor::with_tier(DetectionTier::SpeakerAware);
        assert_eq!(processor.name(), "intelligent_split_speaker");
        assert_eq!(processor.detection_tier(), DetectionTier::SpeakerAware);
    }

    #[test]
    fn test_can_handle_all_intelligent_split_styles() {
        let processor = IntelligentSplitProcessor::new();
        assert!(processor.can_handle(Style::IntelligentSplit));
        assert!(processor.can_handle(Style::IntelligentSplitBasic));
        assert!(processor.can_handle(Style::IntelligentSplitAudio));
        assert!(processor.can_handle(Style::IntelligentSplitSpeaker));

        assert!(!processor.can_handle(Style::Intelligent));
        assert!(!processor.can_handle(Style::Split));
    }
}

