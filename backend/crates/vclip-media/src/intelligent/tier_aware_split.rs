//! Tier-aware split video processing.
//!
//! This module extends the split view processing with tier-specific behavior:
//! - **Basic**: Fixed vertical positioning (current behavior)
//! - **SpeakerAware**: Dynamic per-panel positioning based on face detection
//!
//! For split view styles, the tier primarily affects:
//! 1. Per-panel vertical positioning based on detected face positions
//! 2. Logging and metrics for tier-specific processing
//!
//! # Architecture (Refactored)
//!
//! Detection is now decoupled from rendering:
//! 1. SceneAnalysisService runs detection ONCE per scene (in clip_pipeline)
//! 2. Cached analysis is passed to this processor
//! 3. This processor only handles rendering, not detection
//!
//! When cache is unavailable (fallback), detection runs inline but this
//! should be rare in production.

use std::path::Path;
use tracing::{debug, info, warn};
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::detection_adapter::{
    compute_speaker_split_boxes, compute_split_info_from_detections, extract_split_info,
    get_detections,
};
use super::single_pass_renderer::SinglePassRenderer;
use super::split_renderer;
use crate::clip::extract_segment;
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Tier-aware split processor.
pub struct TierAwareSplitProcessor {
    config: IntelligentCropConfig,
    tier: DetectionTier,
}

impl TierAwareSplitProcessor {
    /// Create a new tier-aware split processor.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self { config, tier }
    }

    /// Create with default configuration.
    pub fn with_tier(tier: DetectionTier) -> Self {
        Self::new(IntelligentCropConfig::default(), tier)
    }

    /// Get the detection tier.
    pub fn tier(&self) -> DetectionTier {
        self.tier
    }

    /// Process a video segment with tier-aware split using SINGLE-PASS encoding.
    ///
    /// This uses SinglePassRenderer to apply all transforms (crop, scale, vstack)
    /// in ONE FFmpeg command, avoiding multiple encode passes.
    ///
    /// # Smart Split Detection
    /// Before splitting, we check if faces appear SIMULTANEOUSLY for at least 3 seconds.
    /// If not (e.g., camera switches between showing one person then another),
    /// we fallback to full-frame intelligent cropping which handles alternating
    /// speakers much better.
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        self.process_with_cached_detections(segment, output, encoding, None).await
    }

    /// Process a video segment with optional cached neural analysis.
    ///
    /// This is the cache-aware entry point that allows skipping expensive ML inference
    /// when cached detections are available.
    ///
    /// # Architecture
    ///
    /// When cached analysis is provided:
    /// 1. Split layout decision uses cached data (no detection)
    /// 2. Vertical positioning uses cached face positions
    /// 3. Speaker-aware split uses cached mouth activity
    ///
    /// When cache is unavailable (fallback):
    /// - Detection runs inline (should be rare in production)
    pub async fn process_with_cached_detections<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
        cached_analysis: Option<&vclip_models::SceneNeuralAnalysis>,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();
        let pipeline_start = std::time::Instant::now();

        info!("[INTELLIGENT_SPLIT] ========================================");
        info!("[INTELLIGENT_SPLIT] START: {:?}", segment);
        info!("[INTELLIGENT_SPLIT] Tier: {:?}", self.tier);
        info!("[INTELLIGENT_SPLIT] Cached analysis: {}", cached_analysis.is_some());

        // Step 1: Get video metadata
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 1/4: Probing video metadata...");

        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "[INTELLIGENT_SPLIT] Step 1/4 DONE in {:.2}s - {}x{} @ {:.2}fps, {:.2}s",
            step_start.elapsed().as_secs_f64(),
            width,
            height,
            fps,
            duration
        );

        // Step 2: Determine split layout using cached analysis or fallback detection
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 2/4: Checking for simultaneous face presence...");

        let split_info = if let Some(analysis) = cached_analysis {
            // Use cached analysis - NO detection needed
            debug!("[INTELLIGENT_SPLIT] Using cached analysis for split decision");
            extract_split_info(analysis, width, height, duration)
        } else {
            // Fallback: run detection through centralized adapter
            warn!("[INTELLIGENT_SPLIT] No cached analysis, running fallback detection");
            let detections = get_detections(
                None,
                segment,
                self.tier,
                0.0,
                duration,
                width,
                height,
                fps,
            )
            .await?;
            compute_split_info_from_detections(&detections, width, height, duration, self.config.fps_sample)
        };

        info!(
            "[INTELLIGENT_SPLIT] Step 2/4 DONE in {:.2}s - should_split: {}",
            step_start.elapsed().as_secs_f64(),
            split_info.should_split
        );

        if !split_info.should_split {
            info!("[INTELLIGENT_SPLIT] Alternating/single-face detected → using full-frame tracking");
            let cropper = super::tier_aware_cropper::TierAwareIntelligentCropper::new(
                self.config.clone(),
                self.tier,
            );
            cropper
                .process_with_cached_detections(segment, output, encoding, cached_analysis)
                .await?;

            // Generate thumbnail
            let thumb_path = output.with_extension("jpg");
            if let Err(e) = generate_thumbnail(output, &thumb_path).await {
                warn!("[INTELLIGENT_SPLIT] Failed to generate thumbnail: {}", e);
            }

            info!("[INTELLIGENT_SPLIT] ========================================");
            info!(
                "[INTELLIGENT_SPLIT] COMPLETE (full-frame) in {:.2}s",
                pipeline_start.elapsed().as_secs_f64()
            );
            return Ok(());
        }

        // Step 3: Speaker-aware split uses cached mouth activity
        if self.tier == DetectionTier::SpeakerAware {
            info!("[INTELLIGENT_SPLIT] Using SpeakerAware processing path");
            if let Some(analysis) = cached_analysis {
                // Use cached analysis for speaker split - NO additional detection
                if let Some((left_box, right_box)) =
                    compute_speaker_split_boxes(analysis, width, height)
                {
                    match split_renderer::render_speaker_split(
                        segment, output, width, height, &left_box, &right_box, encoding,
                    )
                    .await
                    {
                        Ok(()) => {
                            self.finalize_output(output, pipeline_start).await;
                            return Ok(());
                        }
                        Err(e) => {
                            warn!("[INTELLIGENT_SPLIT] Speaker split render failed: {}", e);
                        }
                    }
                }
            }
            // Fallback to standard split if speaker split fails
            warn!("[INTELLIGENT_SPLIT] Speaker split unavailable, using standard split");
        }

        // Step 4: Standard split with vertical positioning from cached analysis
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_SPLIT] Step 3/3: Computing vertical positioning...");

        let left_vertical_bias = split_info.left_vertical_bias(height);
        let right_vertical_bias = split_info.right_vertical_bias(height);

        info!(
            "[INTELLIGENT_SPLIT] Step 3/3 DONE in {:.2}s - left={:.2}, right={:.2}",
            step_start.elapsed().as_secs_f64(),
            left_vertical_bias,
            right_vertical_bias
        );

        // Step 5: Single-pass render (THE ONLY ENCODE)
        info!("[INTELLIGENT_SPLIT] Step 4/4: Single-pass encoding...");
        info!(
            "[INTELLIGENT_SPLIT]   Encoding: {} preset={} crf={}",
            encoding.codec, encoding.preset, encoding.crf
        );

        let renderer = SinglePassRenderer::new(self.config.clone());
        renderer
            .render_split(
                segment,
                output,
                width,
                height,
                left_vertical_bias,
                right_vertical_bias,
                encoding,
            )
            .await?;

        self.finalize_output(output, pipeline_start).await;
        Ok(())
    }

    /// Finalize output: generate thumbnail and log completion.
    async fn finalize_output(&self, output: &Path, pipeline_start: std::time::Instant) {
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            warn!("[INTELLIGENT_SPLIT] Failed to generate thumbnail: {}", e);
        }

        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!("[INTELLIGENT_SPLIT] ========================================");
        info!(
            "[INTELLIGENT_SPLIT] COMPLETE in {:.2}s - {:.2} MB",
            pipeline_start.elapsed().as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );
    }

}

/// Create a tier-aware intelligent split clip from a video file.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Compute vertical positioning per tier
/// 3. `SinglePassRenderer::render_split()` - ONE encode with split filter graph
pub async fn create_tier_aware_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
    progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    // Delegate to cache-aware version with no cache
    create_tier_aware_split_clip_with_cache(
        input,
        output,
        task,
        tier,
        encoding,
        None,
        progress_callback,
    )
    .await
}

/// Create a tier-aware intelligent split clip with optional cached neural analysis.
///
/// This is the cache-aware entry point that allows skipping expensive ML inference
/// when cached detections are available.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Compute vertical positioning per tier (SKIPPED if cache provided)
/// 3. `SinglePassRenderer::render_split()` - ONE encode with split filter graph
pub async fn create_tier_aware_split_clip_with_cache<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
    cached_analysis: Option<&vclip_models::SceneNeuralAnalysis>,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();
    let total_start = std::time::Instant::now();

    info!("========================================================");
    info!("[PIPELINE] INTELLIGENT SPLIT - START");
    info!("[PIPELINE] Source: {:?}", input);
    info!("[PIPELINE] Output: {:?}", output);
    info!("[PIPELINE] Tier: {:?}", tier);
    info!("[PIPELINE] Cached analysis: {}", cached_analysis.is_some());
    info!("[PIPELINE] Encoding: {} crf={}", encoding.codec, encoding.crf);

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    info!("[PIPELINE] Time: {:.2}s to {:.2}s ({:.2}s duration)", start_secs, end_secs, duration);

    // Step 1: Extract segment using STREAM COPY (no encode)
    let segment_path = output.with_extension("segment.mp4");
    info!("[PIPELINE] Step 1/2: Extract segment (STREAM COPY - no encode)...");

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with single-pass render (THE ONLY ENCODE)
    info!("[PIPELINE] Step 2/2: Process segment (SINGLE ENCODE)...");
    
    let config = IntelligentCropConfig::default();
    let processor = TierAwareSplitProcessor::new(config, tier);
    let result = processor
        .process_with_cached_detections(segment_path.as_path(), output, encoding, cached_analysis)
        .await;

    // Cleanup
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!("[PIPELINE] Failed to delete temp segment: {}", e);
        } else {
            info!("[PIPELINE] Cleaned up temp segment");
        }
    }

    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!("========================================================");
    info!(
        "[PIPELINE] INTELLIGENT SPLIT - COMPLETE in {:.2}s - {:.2} MB",
        total_start.elapsed().as_secs_f64(),
        file_size as f64 / 1_000_000.0
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::detection_adapter::compute_vertical_bias;
    use super::super::models::BoundingBox;

    #[test]
    fn test_processor_creation() {
        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::Basic);
        assert_eq!(processor.tier(), DetectionTier::Basic);
    }

    #[test]
    fn test_vertical_bias_computation() {
        // Face at top of frame -> low bias
        let top_face = BoundingBox::new(100.0, 50.0, 100.0, 100.0);
        let bias = compute_vertical_bias(&[top_face], 1080);
        assert!(bias < 0.1, "Top face should have low bias: {}", bias);

        // Face at middle of frame -> medium bias
        let mid_face = BoundingBox::new(100.0, 440.0, 100.0, 100.0);
        let bias = compute_vertical_bias(&[mid_face], 1080);
        assert!(bias > 0.1 && bias < 0.3, "Mid face should have medium bias: {}", bias);

        // Face at bottom of frame -> higher bias (clamped)
        let bottom_face = BoundingBox::new(100.0, 800.0, 100.0, 100.0);
        let bias = compute_vertical_bias(&[bottom_face], 1080);
        assert!(bias >= 0.3, "Bottom face should have higher bias: {}", bias);
    }

    #[test]
    fn test_empty_faces_default_bias() {
        let bias = compute_vertical_bias(&[], 1080);
        assert!((bias - 0.15).abs() < 0.01, "Empty faces should use default bias");
    }
}
