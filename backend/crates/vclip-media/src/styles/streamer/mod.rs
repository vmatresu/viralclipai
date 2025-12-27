//! Streamer style processor (Full View).
//!
//! Creates a 9:16 portrait video with:
//! - Original landscape video centered in the middle
//! - Blurred/zoomed version of the same video as background
//! - Background moves in correlation with the main video (no AI detection)
//!
//! Also supports Top Scenes compilation mode which:
//! - Combines up to 5 selected scenes into a single video
//! - Shows a countdown overlay (5, 4, 3, 2, 1) as scenes transition
//! - Same landscape-in-portrait format as regular Streamer

mod config;
mod filters;
mod pipeline;

use async_trait::async_trait;
use tracing::info;
use vclip_models::{DetectionTier, Style};

use crate::core::observability::ProcessingLogger;
use crate::core::{ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;
use crate::intelligent::parse_timestamp;

use super::utils;
pub use config::StreamerConfig;
pub use pipeline::{
    concatenate_segments, process_top_scenes_from_segments, render_streamer_format,
};

/// Processor for Streamer (full view) video style.
///
/// Creates landscape-in-portrait format with blurred background.
#[derive(Clone, Default)]
pub struct StreamerProcessor {
    config: StreamerConfig,
}

impl StreamerProcessor {
    /// Create a new streamer processor with default config.
    pub fn new() -> Self {
        Self {
            config: StreamerConfig::default(),
        }
    }

    /// Create a new streamer processor with custom config.
    pub fn with_config(config: StreamerConfig) -> Self {
        Self { config }
    }

    /// Get the detection tier (None - no AI detection needed).
    pub fn detection_tier(&self) -> DetectionTier {
        DetectionTier::None
    }
}

#[async_trait]
impl StyleProcessor for StreamerProcessor {
    fn name(&self) -> &'static str {
        "streamer"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::Streamer | Style::StreamerTopScenes)
    }

    async fn validate(
        &self,
        request: &ProcessingRequest,
        ctx: &ProcessingContext,
    ) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        ctx.security.check_resource_limits("ffmpeg")?;

        // Validate streamer params if present
        if let Some(params) = &request.task.streamer_params {
            if params.top_scenes_enabled && params.top_scenes.len() > 5 {
                return Err(crate::error::MediaError::InvalidVideo(
                    "Top Scenes compilation supports a maximum of 5 scenes".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn process(
        &self,
        request: ProcessingRequest,
        ctx: ProcessingContext,
    ) -> MediaResult<ProcessingResult> {
        let is_top_scenes = request.task.style == Style::StreamerTopScenes;
        let style_name = if is_top_scenes {
            "streamer_top_scenes"
        } else {
            "streamer"
        };

        let timer = ctx.metrics.start_timer("streamer_processing");
        let logger = ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            style_name.to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        // Get streamer params
        let params = request.task.streamer_params.clone().unwrap_or_default();

        if is_top_scenes && params.top_scenes_enabled && !params.top_scenes.is_empty() {
            info!(
                "[STREAMER] Processing Top Scenes compilation with {} scenes",
                params.top_scenes.len()
            );
            pipeline::process_top_scenes(
                request.input_path.as_ref(),
                request.output_path.as_ref(),
                &request.encoding,
                &params,
                &self.config,
                request.watermark.as_ref(),
            )
            .await?;
        } else {
            info!("[STREAMER] Processing single scene with landscape-in-portrait format");
            pipeline::process_single(
                request.input_path.as_ref(),
                request.output_path.as_ref(),
                &request.task,
                &request.encoding,
                &self.config,
                request.watermark.as_ref(),
            )
            .await?;
        }

        let processing_time = timer.elapsed();

        let file_size = tokio::fs::metadata(&request.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration = parse_timestamp(&request.task.end).unwrap_or(30.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0);

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(utils::thumbnail_path(&request.output_path).into()),
            duration_seconds: duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics
            .increment_counter("processing_completed", &[("style", style_name)]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", style_name)],
        );

        timer.success();
        logger.log_completion(&result);

        Ok(result)
    }

    fn estimate_complexity(
        &self,
        request: &ProcessingRequest,
    ) -> crate::core::ProcessingComplexity {
        let duration = parse_timestamp(&request.task.end).unwrap_or(30.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0);

        // Streamer is fast (no AI detection), but has more complex filter graph
        let mut complexity = utils::estimate_complexity(duration, false);
        complexity.estimated_time_ms = (complexity.estimated_time_ms as f64 * 1.2) as u64;
        complexity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streamer_processor_creation() {
        let processor = StreamerProcessor::new();
        assert_eq!(processor.name(), "streamer");
        assert!(processor.can_handle(Style::Streamer));
        assert!(processor.can_handle(Style::StreamerTopScenes));
        assert!(!processor.can_handle(Style::StreamerSplit));
        assert_eq!(processor.detection_tier(), DetectionTier::None);
    }

    #[test]
    fn test_streamer_is_fast() {
        assert!(Style::Streamer.is_fast());
        assert!(Style::StreamerTopScenes.is_fast());
        assert_eq!(Style::Streamer.detection_tier(), DetectionTier::None);
        assert_eq!(
            Style::StreamerTopScenes.detection_tier(),
            DetectionTier::None
        );
    }

    #[test]
    fn test_config_defaults() {
        let config = StreamerConfig::default();
        assert_eq!(config.output_width, 1080);
        assert_eq!(config.output_height, 1920);
        assert!(config.background_blur > 0.0);
        assert!(config.background_zoom > 1.0);
    }
}
