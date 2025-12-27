//! Visual activity-based intelligent split pipeline.
//!
//! This module provides the processing pipeline for visual-based split styles:
//! - `IntelligentSplitMotion` - split + frame differencing activity detection
//! - `IntelligentSplitActivity` - split + full visual activity tracking
//!
//! Unlike audio-based split styles, these work with any audio format.
//!
//! # Pipeline
//!
//! The visual activity split uses the same base split infrastructure but applies
//! motion-based activity detection instead of stereo audio analysis.

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::split::{IntelligentSplitProcessor as BaseSplitProcessor, SplitLayout};
use crate::clip::extract_segment;
use crate::error::MediaResult;
use crate::thumbnail::generate_thumbnail;

/// Visual activity split processor for motion-based intelligent split styles.
///
/// Uses visual cues (motion, size changes) instead of stereo audio analysis.
/// Works with any audio format.
///
/// This processor wraps the base `IntelligentSplitProcessor` and adds visual
/// activity detection tier information.
pub struct VisualActivitySplitProcessor {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    base_processor: BaseSplitProcessor,
}

impl VisualActivitySplitProcessor {
    /// Create a new visual activity split processor.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier) -> Self {
        Self {
            base_processor: BaseSplitProcessor::new(config.clone()),
            config,
            tier,
        }
    }

    /// Create with default configuration for MotionAware tier.
    pub fn motion_aware() -> Self {
        Self::new(IntelligentCropConfig::default(), DetectionTier::MotionAware)
    }

    /// Get the detection tier.
    pub fn tier(&self) -> DetectionTier {
        self.tier
    }

    /// Get the configuration.
    #[allow(dead_code)]
    pub fn config(&self) -> &IntelligentCropConfig {
        &self.config
    }

    /// Process a pre-cut video segment with visual activity-based intelligent split.
    ///
    /// For visual activity styles, we use the base split processor but the tier
    /// information can be used upstream for different face selection strategies.
    ///
    /// # Arguments
    /// * `input` - Path to the pre-cut video segment
    /// * `output` - Path for the output file
    /// * `encoding` - Encoding configuration
    pub async fn process<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<SplitLayout> {
        let input = input.as_ref();
        let output = output.as_ref();

        info!(
            "Starting visual activity split (tier: {:?}) for {:?}",
            self.tier, input
        );

        // Use the base split processor
        // In the future, the tier could influence face selection within panels
        let result = self.base_processor.process(input, output, encoding).await?;

        info!("Visual activity split complete: {:?}", output);
        Ok(result)
    }
}

/// Create a visual activity intelligent split clip from a video file.
///
/// This is the main entry point for IntelligentSplitMotion and IntelligentSplitActivity styles.
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `tier` - Detection tier (MotionAware)
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_visual_activity_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    tier: DetectionTier,
    encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    // Step 1: Extract segment to temporary file
    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for visual activity split: {:.2}s - {:.2}s (tier: {:?})",
        start_secs, end_secs, tier
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Apply visual activity split processing
    let config = IntelligentCropConfig::default();
    let processor = VisualActivitySplitProcessor::new(config, tier);
    let output_buf = output.to_path_buf();
    let result = processor
        .process(&segment_path, &output_buf, encoding)
        .await;

    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!(
                "Failed to delete temporary segment file {}: {}",
                segment_path.display(),
                e
            );
        } else {
            info!("Deleted temporary segment: {}", segment_path.display());
        }
    }

    // Step 4: Generate thumbnail
    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        tracing::warn!("Failed to generate thumbnail: {}", e);
    }

    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_creation() {
        let processor = VisualActivitySplitProcessor::motion_aware();
        assert_eq!(processor.tier(), DetectionTier::MotionAware);
    }
}
