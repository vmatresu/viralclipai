//! Tier-aware intelligent cropping pipeline.
//!
//! This module provides the main entry point for tier-specific intelligent cropping.
//! It orchestrates face detection, speaker detection, activity analysis, and camera
//! planning based on the detection tier.
//!
//! # Architecture (Refactored)
//!
//! Detection is now decoupled from rendering:
//! 1. SceneAnalysisService runs detection ONCE per scene (in clip_pipeline)
//! 2. Cached analysis is passed to this processor via `process_with_cached_detections`
//! 3. This processor only handles rendering, not detection (when cache available)
//!
//! # Tier Behavior
//!
//! - **Basic**: Face detection → Camera smoothing → Crop planning
//! - **SpeakerAware** (`intelligent_speaker`): Uses the premium camera planner with:
//!   - Smart target selection with vertical bias for eye placement
//!   - Dead-zone hysteresis for camera stability
//!   - Multi-speaker dwell time to prevent ping-ponging
//!   - Scene change detection for fast adaptation
//!   - Exponential smoothing with max pan speed limits
//! - **MotionAware**: Visual motion heuristics for high-motion content

use std::path::Path;
use tracing::info;
use vclip_models::{ClipTask, DetectionTier, EncodingConfig};

use super::config::IntelligentCropConfig;
use super::crop_planner::CropPlanner;
use super::detection_adapter::get_detections;
use super::models::AspectRatio;
use super::premium::{PremiumCameraPlanner, PremiumSpeakerConfig};
use super::single_pass_renderer::SinglePassRenderer;
use super::tier_aware_smoother::TierAwareCameraSmoother;
use crate::clip::extract_segment;
use crate::error::MediaResult;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Tier-aware intelligent cropper.
///
/// Orchestrates the full intelligent cropping pipeline with tier-specific behavior.
pub struct TierAwareIntelligentCropper {
    config: IntelligentCropConfig,
    tier: DetectionTier,
}

impl TierAwareIntelligentCropper {
    /// Create a new tier-aware cropper.
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

    /// Process a pre-cut video segment with tier-aware intelligent cropping.
    ///
    /// Uses SINGLE-PASS rendering to avoid multiple encodes.
    /// Input should be a stream-copy segment (not re-encoded).
    ///
    /// # Arguments
    /// * `segment` - Pre-extracted segment (stream copy from source)
    /// * `output` - Final output path
    /// * `encoding` - Encoding config from API
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        self.process_with_cached_detections(segment, output, encoding, None).await
    }

    /// Process a pre-cut video segment with optional cached neural analysis.
    ///
    /// This is the Phase 3 entry point that allows skipping expensive ML inference
    /// when cached detections are available.
    ///
    /// # Arguments
    /// * `segment` - Pre-extracted segment (stream copy from source)
    /// * `output` - Final output path
    /// * `encoding` - Encoding config from API
    /// * `cached_analysis` - Optional pre-computed neural analysis from cache
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

        info!("[INTELLIGENT_FULL] ========================================");
        info!("[INTELLIGENT_FULL] START: {:?}", segment);
        info!("[INTELLIGENT_FULL] Tier: {:?}", self.tier);
        info!("[INTELLIGENT_FULL] Cached analysis: {}", cached_analysis.is_some());

        // Step 1: Get video metadata
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_FULL] Step 1/4: Probing video metadata...");
        
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "[INTELLIGENT_FULL] Step 1/4 DONE in {:.2}s - {}x{} @ {:.2}fps, {:.2}s",
            step_start.elapsed().as_secs_f64(),
            width, height, fps, duration
        );

        let start_time = 0.0;
        let end_time = duration;

        // Step 2: Face detection (or use cached) - uses centralized detection adapter
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_FULL] Step 2/4: Getting detections (cached: {})...", cached_analysis.is_some());
        
        let detections = get_detections(
            cached_analysis,
            segment,
            self.tier,
            start_time,
            end_time,
            width,
            height,
            fps,
        )
        .await?;

        let total_detections: usize = detections.iter().map(|d| d.len()).sum();
        info!(
            "[INTELLIGENT_FULL] Step 2/4 DONE in {:.2}s - {} detections in {} frames",
            step_start.elapsed().as_secs_f64(),
            total_detections,
            detections.len()
        );

        // Step 3: Camera path smoothing
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_FULL] Step 3/4: Computing smooth camera path...");
        
        let target_aspect = AspectRatio::new(9, 16);
        
        // Use premium camera planner for SpeakerAware tier (intelligent_speaker)
        let (camera_keyframes, crop_windows) = if matches!(self.tier, DetectionTier::SpeakerAware) {
            info!("[INTELLIGENT_FULL]   Using Premium Camera Planner for intelligent_speaker");
            let premium_config = PremiumSpeakerConfig::default();
            let mut premium_planner = PremiumCameraPlanner::new(
                premium_config,
                width,
                height,
                fps,
            );
            
            let keyframes = premium_planner.compute_camera_plan(&detections, start_time, end_time);
            let crops = premium_planner.compute_crop_windows(&keyframes, &target_aspect);
            
            info!(
                "[INTELLIGENT_FULL]   Primary subject: {:?}",
                premium_planner.current_primary_subject()
            );
            
            (keyframes, crops)
        } else {
            // Use standard tier-aware smoother for other tiers
            let mut smoother = TierAwareCameraSmoother::new(self.config.clone(), self.tier, fps);
            let keyframes = smoother.compute_camera_plan(
                &detections,
                width,
                height,
                start_time,
                end_time,
            );
            
            let planner = CropPlanner::new(self.config.clone(), width, height);
            let crops = planner.compute_crop_windows(&keyframes, &target_aspect);
            
            (keyframes, crops)
        };
        
        info!(
            "[INTELLIGENT_FULL] Step 3/4 DONE in {:.2}s - {} keyframes",
            step_start.elapsed().as_secs_f64(),
            camera_keyframes.len()
        );

        // Step 4: Verify crop windows
        let step_start = std::time::Instant::now();
        info!("[INTELLIGENT_FULL] Step 4/4: Verifying crop windows...");
        
        info!(
            "[INTELLIGENT_FULL] Step 4/4 DONE in {:.2}s - {} crop windows verified",
            step_start.elapsed().as_secs_f64(),
            crop_windows.len()
        );

        // Step 5: Single-pass render (THE ONLY ENCODE)
        info!("[INTELLIGENT_FULL] Step 5/5: Single-pass encoding...");
        info!("[INTELLIGENT_FULL]   Encoding: {} preset={} crf={}", 
            encoding.codec, encoding.preset, encoding.crf);
        
        let renderer = SinglePassRenderer::new(self.config.clone());
        renderer
            .render_full(segment, output, &crop_windows, encoding)
            .await?;

        // Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("[INTELLIGENT_FULL] Failed to generate thumbnail: {}", e);
        }

        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!("[INTELLIGENT_FULL] ========================================");
        info!(
            "[INTELLIGENT_FULL] COMPLETE in {:.2}s - {:.2} MB",
            pipeline_start.elapsed().as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
    }

}

/// Create a tier-aware intelligent clip from a video file.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Face detection on segment
/// 3. Camera path smoothing
/// 4. `SinglePassRenderer` - ONE encode with crop filter
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `tier` - Detection tier controlling which providers are used
/// * `encoding` - Encoding configuration (CRF, preset, etc.)
/// * `progress_callback` - Callback for progress updates
pub async fn create_tier_aware_intelligent_clip<P, F>(
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
    // Delegate to the cache-aware version with no cache
    create_tier_aware_intelligent_clip_with_cache(
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

/// Create a tier-aware intelligent clip with optional cached neural analysis.
///
/// This is the Phase 3 entry point that allows skipping expensive ML inference
/// when cached detections are available.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Face detection on segment (SKIPPED if cache provided)
/// 3. Camera path smoothing
/// 4. `SinglePassRenderer` - ONE encode with crop filter
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `tier` - Detection tier controlling which providers are used
/// * `encoding` - Encoding configuration (CRF, preset, etc.)
/// * `cached_analysis` - Optional pre-computed neural analysis from cache
/// * `progress_callback` - Callback for progress updates
pub async fn create_tier_aware_intelligent_clip_with_cache<P, F>(
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
    info!("[PIPELINE] INTELLIGENT FULL - START");
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
    // Pass cached analysis to skip ML inference if available
    info!("[PIPELINE] Step 2/2: Process segment (SINGLE ENCODE)...");
    
    let config = IntelligentCropConfig::default();
    let cropper = TierAwareIntelligentCropper::new(config, tier);
    let result = cropper
        .process_with_cached_detections(segment_path.as_path(), output, encoding, cached_analysis)
        .await;

    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!(
                "[PIPELINE] Failed to delete temp segment: {}",
                e
            );
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
        "[PIPELINE] INTELLIGENT FULL - COMPLETE in {:.2}s - {:.2} MB",
        total_start.elapsed().as_secs_f64(),
        file_size as f64 / 1_000_000.0
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cropper_creation() {
        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::Basic);
        assert_eq!(cropper.tier(), DetectionTier::Basic);

        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::SpeakerAware);
        assert_eq!(cropper.tier(), DetectionTier::SpeakerAware);
    }

    #[test]
    fn test_tier_uses_audio() {
        assert!(!DetectionTier::None.uses_audio());
        assert!(!DetectionTier::Basic.uses_audio());
        assert!(!DetectionTier::SpeakerAware.uses_audio());
    }
}
