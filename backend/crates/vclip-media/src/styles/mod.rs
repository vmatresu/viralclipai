//! Style processors for video processing.
//!
//! Each video style (Original, Split, LeftFocus, etc.) has its own processor
//! implementing the StyleProcessor trait. This ensures complete separation
//! and testability of style-specific logic.
//!
//! Intelligent styles are instantiated with their appropriate `DetectionTier`:
//! - `Basic`: YuNet face detection only
//! - `SpeakerAware`: YuNet + audio + face activity
//! - `Motion/Activity`: Visual motion + activity

use std::path::Path;

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{StyleProcessor, StyleProcessorFactory as StyleProcessorFactoryTrait};

pub mod original;
pub mod split;
pub mod split_fast;
pub mod left_focus;
pub mod right_focus;
pub mod intelligent;
pub mod intelligent_split;

/// Factory for creating style processors.
/// Implements dependency injection for testing and flexibility.
#[derive(Clone)]
pub struct StyleProcessorFactory {
    // Configuration can be added here for different environments
}

impl StyleProcessorFactory {
    /// Create a new factory.
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl StyleProcessorFactoryTrait for StyleProcessorFactory {
    /// Create a processor for the given style.
    ///
    /// Processors are instantiated with the appropriate `DetectionTier` based on
    /// the style's tier mapping.
    async fn create_processor(&self, style: Style) -> MediaResult<Box<dyn StyleProcessor>> {
        match style {
            // Static/fast styles
            Style::Original => Ok(Box::new(original::OriginalProcessor::new())),
            Style::Split => Ok(Box::new(split::SplitProcessor::new())),
            Style::SplitFast => Ok(Box::new(split_fast::SplitFastProcessor::new())),
            Style::LeftFocus => Ok(Box::new(left_focus::LeftFocusProcessor::new())),
            Style::RightFocus => Ok(Box::new(right_focus::RightFocusProcessor::new())),

            // Intelligent single-view styles (tier-aware, audio + activity)
            Style::Intelligent | Style::IntelligentBasic => {
                Ok(Box::new(intelligent::IntelligentProcessor::new()))
            }
            Style::IntelligentSpeaker => {
                Ok(Box::new(intelligent::IntelligentProcessor::with_tier(
                    vclip_models::DetectionTier::SpeakerAware,
                )))
            }

            // Intelligent single-view styles (tier-aware, visual-based)
            Style::IntelligentMotion => {
                Ok(Box::new(intelligent::IntelligentProcessor::with_tier(
                    vclip_models::DetectionTier::MotionAware,
                )))
            }
            Style::IntelligentActivity => {
                Ok(Box::new(intelligent::IntelligentProcessor::with_tier(
                    vclip_models::DetectionTier::ActivityAware,
                )))
            }

            // Intelligent split-view styles (tier-aware, audio + activity)
            Style::IntelligentSplit | Style::IntelligentSplitBasic => {
                Ok(Box::new(intelligent_split::IntelligentSplitProcessor::new()))
            }
            Style::IntelligentSplitSpeaker => {
                Ok(Box::new(intelligent_split::IntelligentSplitProcessor::with_tier(
                    vclip_models::DetectionTier::SpeakerAware,
                )))
            }

            // Intelligent split-view styles (tier-aware, visual-based)
            Style::IntelligentSplitMotion => {
                Ok(Box::new(intelligent_split::IntelligentSplitProcessor::with_tier(
                    vclip_models::DetectionTier::MotionAware,
                )))
            }
            Style::IntelligentSplitActivity => {
                Ok(Box::new(intelligent_split::IntelligentSplitProcessor::with_tier(
                    vclip_models::DetectionTier::ActivityAware,
                )))
            }
        }
    }
}

impl Default for StyleProcessorFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions shared across style processors.
pub mod utils {
    use super::*;
    use std::time::Instant;
    use crate::clip::create_clip;
    use crate::core::observability::ProcessingLogger;
    use crate::intelligent::parse_timestamp;

    /// Validate that input and output paths are accessible.
    pub fn validate_paths(input: &Path, output: &Path) -> MediaResult<()> {
        if !input.exists() {
            return Err(crate::error::MediaError::InvalidVideo(
                format!("Input file does not exist: {}", input.display())
            ));
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(crate::error::MediaError::InvalidVideo(
                    format!("Output directory does not exist: {}", parent.display())
                ));
            }
        }

        Ok(())
    }

    /// Generate thumbnail path from output path.
    pub fn thumbnail_path(output: &Path) -> std::path::PathBuf {
        output.with_extension("jpg")
    }

    /// Calculate processing complexity based on video properties.
    pub fn estimate_complexity(
        duration_seconds: f64,
        requires_intelligence: bool
    ) -> crate::core::ProcessingComplexity {
        let base_time = if requires_intelligence { 60_000 } else { 30_000 }; // ms
        let duration_factor = (duration_seconds / 60.0).max(1.0); // Scale by duration

        crate::core::ProcessingComplexity {
            estimated_time_ms: (base_time as f64 * duration_factor) as u64,
            cpu_usage: if requires_intelligence { 0.8 } else { 0.3 },
            memory_mb: if requires_intelligence { 512 } else { 128 },
            temp_space_mb: 256,
        }
    }

    /// Execute a basic FFmpeg-driven style using the shared clip pipeline.
    /// Centralizes logging, metrics, padding, and thumbnail handling for static styles.
    pub async fn run_basic_style(
        request: crate::core::ProcessingRequest,
        ctx: crate::core::ProcessingContext,
        style_label: &'static str,
    ) -> crate::error::MediaResult<crate::core::ProcessingResult> {
        let timer = ctx.metrics.start_timer(&format!("{style_label}_processing"));
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            style_label.to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        let start_time = Instant::now();

        create_clip(
            request.input_path.as_ref(),
            request.output_path.as_ref(),
            &request.task,
            &request.encoding,
            |_progress| {},
        )
        .await?;

        let processing_time = start_time.elapsed();
        let file_size = tokio::fs::metadata(&*request.output_path)
            .await
            .map(|m| m.len())?;
        let duration = parse_timestamp(&request.task.end).unwrap_or(0.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0)
            + request.task.pad_before
            + request.task.pad_after;

        let thumbnail_path = thumbnail_path(&request.output_path);

        let result = crate::core::ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(thumbnail_path.into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics
            .increment_counter("processing_completed", &[("style", style_label)]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", style_label)],
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }
}
