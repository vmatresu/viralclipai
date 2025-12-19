//! Intelligent activity-based split style processor.
//!
//! Handles "Smart Split (Activity)" style which dynamically switches between
//! full-screen and split-screen based on the number of active speakers.
//!
//! This behavior is tier-aware:
//! - `SpeakerAware`: Uses full face+mouth activity (visual only)
//! - `MotionAware`: Uses motion clusters
//! - `Basic`: Uses face detection presence

use async_trait::async_trait;
use tracing::info;
use vclip_models::{DetectionTier, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;
use crate::intelligent::parse_timestamp;

use super::utils;

/// Processor for intelligent activity split style.
#[derive(Clone)]
pub struct IntelligentSplitActivityProcessor {
    tier: DetectionTier,
}

impl IntelligentSplitActivityProcessor {
    pub fn new(tier: DetectionTier) -> Self {
        Self { tier }
    }
}

#[async_trait]
impl StyleProcessor for IntelligentSplitActivityProcessor {
    fn name(&self) -> &'static str {
        match self.tier {
            DetectionTier::SpeakerAware => "intelligent_split_activity_speaker",
            DetectionTier::MotionAware => "intelligent_split_activity_motion",
            _ => "intelligent_split_activity",
        }
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::IntelligentSplitActivity)
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
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
        let timer = ctx.metrics.start_timer("intelligent_split_activity_processing");
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

        crate::intelligent::activity_split::create_activity_split_clip(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            self.tier,
            &request.encoding,
            request.watermark.as_ref(),
            |_progress| {
                // Progress callback
            },
        )
        .await?;

        let processing_time = timer.elapsed();

        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration = parse_timestamp(&request.task.end).unwrap_or(0.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0)
            + request.task.pad_before
            + request.task.pad_after;

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
        // High complexity style
        let duration = parse_timestamp(&request.task.end).unwrap_or(30.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0);
        
        let multiplier = 2.0;

        let mut complexity = utils::estimate_complexity(duration, true);
        complexity.estimated_time_ms = (complexity.estimated_time_ms as f64 * multiplier) as u64;
        complexity
    }
}
