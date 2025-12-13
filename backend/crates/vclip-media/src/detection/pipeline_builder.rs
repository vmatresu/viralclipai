//! Pipeline builder for tier-specific detection configurations.
//!
//! The `PipelineBuilder` creates detection pipelines appropriate for each
//! `DetectionTier`. Each tier is strict: if a required detector is unavailable
//! or fails, the pipeline returns an error rather than degrading quality.
//!
//! # Architecture
//!
//! Pipeline implementations are modular and located in the `pipelines` submodule:
//! - `NonePipeline` - Heuristic positioning only (fastest)
//! - `BasicPipeline` - YuNet face detection
//! - `SpeakerAwarePipeline` - YuNet + FaceMesh mouth activity
//! - `MotionAwarePipeline` - Visual motion heuristics (no NN)

use tracing::info;
use vclip_models::DetectionTier;

use super::pipeline::DetectionPipeline;
use super::pipelines::{BasicPipeline, MotionAwarePipeline, NonePipeline, SpeakerAwarePipeline};
use crate::error::MediaResult;

/// Builder for creating detection pipelines based on tier.
///
/// # Example
///
/// ```ignore
/// let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware).build()?;
/// let result = pipeline.analyze(video_path, 0.0, 30.0).await?;
/// ```
pub struct PipelineBuilder {
    requested_tier: DetectionTier,
}

impl PipelineBuilder {
    /// Create a builder for the specified tier.
    pub fn for_tier(tier: DetectionTier) -> Self {
        Self {
            requested_tier: tier,
        }
    }

    /// Build the detection pipeline.
    ///
    /// Returns a boxed trait object that implements `DetectionPipeline`.
    pub fn build(self) -> MediaResult<Box<dyn DetectionPipeline>> {
        match self.requested_tier {
            DetectionTier::None => {
                info!("Building None tier pipeline (heuristic only)");
                Ok(Box::new(NonePipeline))
            }
            DetectionTier::Basic => {
                info!("Building Basic tier pipeline (YuNet face detection)");
                Ok(Box::new(BasicPipeline::new()))
            }
            DetectionTier::SpeakerAware => {
                info!("Building SpeakerAware tier pipeline (YuNet + FaceMesh visual activity)");
                Ok(Box::new(SpeakerAwarePipeline::new()))
            }
            DetectionTier::MotionAware => {
                info!("Building MotionAware tier pipeline (motion heuristics)");
                Ok(Box::new(MotionAwarePipeline::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_builder_none() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::None)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::None);
        assert_eq!(pipeline.name(), "none");
    }

    #[test]
    fn test_pipeline_builder_basic() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::Basic)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::Basic);
        assert_eq!(pipeline.name(), "basic");
    }

    #[test]
    fn test_pipeline_builder_speaker_aware() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::SpeakerAware);
        assert_eq!(pipeline.name(), "speaker_aware");
    }

    #[test]
    fn test_pipeline_builder_motion_aware() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::MotionAware)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::MotionAware);
        assert_eq!(pipeline.name(), "motion_aware");
    }

    #[test]
    fn test_all_tiers_can_build() {
        for tier in DetectionTier::ALL {
            let result = PipelineBuilder::for_tier(*tier).build();
            assert!(
                result.is_ok(),
                "Failed to build pipeline for tier {:?}",
                tier
            );
        }
    }
}
