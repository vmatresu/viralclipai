//! Split Fast style processor.
//!
//! Uses the FastSplitEngine for heuristic-based video splitting without
//! AI face detection. This is the fastest split-view option.

use std::time::Instant;

use async_trait::async_trait;
use tracing::info;

use crate::core::{ProcessingComplexity, ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor};
use crate::error::MediaResult;
use crate::intelligent::{parse_timestamp, FastSplitEngine};
use crate::probe::probe_video;
use vclip_models::Style;
use super::utils;

/// Split Fast processor - uses heuristic positioning only.
pub struct SplitFastProcessor;

impl SplitFastProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SplitFastProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StyleProcessor for SplitFastProcessor {
    fn name(&self) -> &'static str {
        "split_fast"
    }

    fn can_handle(&self, style: Style) -> bool {
        matches!(style, Style::SplitFast)
    }

    async fn validate(&self, request: &ProcessingRequest, _ctx: &ProcessingContext) -> MediaResult<()> {
        utils::validate_paths(&request.input_path, &request.output_path)?;
        Ok(())
    }

    fn estimate_complexity(&self, request: &ProcessingRequest) -> ProcessingComplexity {
        let duration = parse_timestamp(&request.task.end).unwrap_or(30.0)
            - parse_timestamp(&request.task.start).unwrap_or(0.0);
        // Fast processing - no AI detection
        utils::estimate_complexity(duration, false)
    }

    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult> {
        let timer = ctx.metrics.start_timer("split_fast_processing");
        let logger = crate::core::observability::ProcessingLogger::new(
            ctx.request_id.clone(),
            ctx.user_id.clone(),
            "split_fast".to_string(),
        );

        logger.log_start(&request.input_path, &request.output_path);

        let start_time = Instant::now();
        let start_secs = parse_timestamp(&request.task.start).unwrap_or(0.0);
        let end_secs = parse_timestamp(&request.task.end).unwrap_or(30.0);
        let duration = end_secs - start_secs;

        // Create segment path as PathBuf
        let segment_path = request.output_path.with_file_name(
            format!("{}.segment.mp4", request.output_path.file_stem().unwrap_or_default().to_string_lossy())
        );

        // Extract segment first - convert Arc<Path> to Path refs
        crate::clip::extract_segment(
            request.input_path.as_ref(),
            segment_path.as_path(),
            (start_secs - request.task.pad_before).max(0.0),
            duration + request.task.pad_before + request.task.pad_after,
        ).await?;

        // Process with FastSplitEngine - convert Arc<Path> to Path ref
        let output_pathbuf = request.output_path.to_path_buf();
        let engine = FastSplitEngine::new();
        engine.process(&segment_path, &output_pathbuf, &request.encoding).await?;

        // Cleanup segment
        if segment_path.exists() {
            tokio::fs::remove_file(&segment_path).await.ok();
        }

        // Get output info
        let processing_time = start_time.elapsed();
        let video_info = probe_video(&*request.output_path).await?;
        let file_size = tokio::fs::metadata(&*request.output_path).await?.len();

        let thumbnail_path = utils::thumbnail_path(&request.output_path);

        let result = ProcessingResult {
            output_path: request.output_path.clone(),
            thumbnail_path: Some(thumbnail_path.into()),
            duration_seconds: video_info.duration,
            file_size_bytes: file_size,
            processing_time_ms: processing_time.as_millis() as u64,
            metadata: Default::default(),
        };

        ctx.metrics.increment_counter("processing_completed", &[("style", "split_fast")]);
        ctx.metrics.record_histogram(
            "processing_duration_ms",
            processing_time.as_millis() as f64,
            &[("style", "split_fast")]
        );

        timer.success();
        logger.log_completion(&result);

        info!(
            "Split fast complete in {:.2}s: {:?}",
            processing_time.as_secs_f64(),
            request.output_path
        );

        Ok(result)
    }
}
