//! None tier pipeline - heuristic positioning only.

use async_trait::async_trait;
use std::path::Path;
use vclip_models::DetectionTier;

use crate::detection::pipeline::{DetectionPipeline, DetectionResult};
use crate::error::MediaResult;
use crate::probe::probe_video;

/// Pipeline for `DetectionTier::None` - no detection, heuristic positioning only.
///
/// This is the fastest tier, using center-based positioning without any ML inference.
pub struct NonePipeline;

#[async_trait]
impl DetectionPipeline for NonePipeline {
    async fn analyze(
        &self,
        video_path: &Path,
        _start_time: f64,
        _end_time: f64,
    ) -> MediaResult<DetectionResult> {
        let video_info = probe_video(video_path).await?;

        Ok(DetectionResult {
            frames: vec![],
            speaker_segments: None,
            tier_used: DetectionTier::None,
            width: video_info.width,
            height: video_info.height,
            fps: video_info.fps,
            duration: video_info.duration,
        })
    }

    fn tier(&self) -> DetectionTier {
        DetectionTier::None
    }

    fn name(&self) -> &'static str {
        "none"
    }
}
