//! Scene Analysis Service - Centralized detection with caching.
//!
//! This module provides a unified interface for running neural analysis on video scenes.
//! It ensures detection runs ONCE per scene and caches results for all styles to consume.
//!
//! # Architecture
//!
//! The Scene Analysis Service decouples detection from rendering:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Scene Processing                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  1. SceneAnalysisService.ensure_analysis_cached()               │
//! │     - Checks if analysis exists for required tier               │
//! │     - If not, runs detection ONCE and caches                    │
//! │     - Uses Redis lock to prevent duplicate computation          │
//! │                                                                 │
//! │  2. Process all styles in parallel                              │
//! │     - Each style fetches cached analysis                        │
//! │     - No duplicate detection - all use same cache               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! // Before processing styles for a scene, ensure analysis is cached
//! let analysis_service = SceneAnalysisService::new(ctx);
//!
//! // This runs detection ONCE if not cached, or returns immediately if cached
//! analysis_service.ensure_analysis_cached(
//!     user_id,
//!     video_id,
//!     scene_id,
//!     video_path,
//!     start_time,
//!     end_time,
//!     required_tier,
//! ).await?;
//!
//! // Now all styles can process in parallel using cached analysis
//! ```

use std::path::Path;
use tracing::{debug, info, warn};
use vclip_models::{DetectionTier, FrameAnalysis, SceneNeuralAnalysis};

use crate::cinematic_signals::{compute_cinematic_signals, CinematicSignalOptions};
use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;

/// Service for managing scene-level neural analysis.
///
/// Ensures detection runs exactly ONCE per scene, regardless of how many
/// styles need the results. Uses the neural cache service for storage
/// and Redis locking for concurrency control.
pub struct SceneAnalysisService<'a> {
    ctx: &'a EnhancedProcessingContext,
}

impl<'a> SceneAnalysisService<'a> {
    /// Create a new scene analysis service.
    pub fn new(ctx: &'a EnhancedProcessingContext) -> Self {
        Self { ctx }
    }

    /// Ensure neural analysis is cached for a scene at the required tier.
    ///
    /// This is the main entry point. It:
    /// 1. Checks if analysis exists at the required tier
    /// 2. If not, runs detection and caches the result
    /// 3. Uses Redis locking to prevent duplicate computation
    ///
    /// After this call, all styles can safely fetch cached analysis.
    ///
    /// # Arguments
    /// * `user_id` - User ID for cache key
    /// * `video_id` - Video ID for cache key
    /// * `scene_id` - Scene ID for cache key
    /// * `video_path` - Path to the video segment
    /// * `start_time` - Start time in seconds
    /// * `end_time` - End time in seconds
    /// * `required_tier` - Minimum detection tier required
    ///
    /// # Returns
    /// The cached analysis (either existing or newly computed)
    pub async fn ensure_analysis_cached(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
        required_tier: DetectionTier,
    ) -> WorkerResult<SceneNeuralAnalysis> {
        // Use get_or_compute which handles locking and caching atomically
        let video_path = video_path.to_path_buf();

        let (analysis, stored_bytes) = self
            .ctx
            .neural_cache
            .get_or_compute(user_id, video_id, scene_id, required_tier, || {
                let video_path = video_path.clone();
                let user_id = user_id.to_string();
                let video_id = video_id.to_string();
                let handle = tokio::runtime::Handle::current();
                
                async move {
                    // Offload heavy CPU work (neural analysis) to a blocking thread
                    // to avoid stalling the async runtime.
                    // We use handle.block_on to run the inner async function (which calls blocking OpenCV code)
                    // on the blocking thread.
                    tokio::task::spawn_blocking(move || {
                        handle.block_on(run_detection(
                            &video_path,
                            &user_id,
                            &video_id,
                            scene_id,
                            start_time,
                            end_time,
                            required_tier,
                        ))
                    })
                    .await
                    .map_err(|e| WorkerError::job_failed(format!("Blocking task join error: {}", e)))?
                }
            })
            .await?;

        // Track storage accounting if we stored new data
        if let Some(bytes) = stored_bytes {
            info!(
                user_id = %user_id,
                video_id = %video_id,
                scene_id = scene_id,
                frames = analysis.frames.len(),
                stored_bytes = bytes,
                tier = %required_tier,
                "Neural analysis computed and cached"
            );

            let storage_repo = vclip_firestore::StorageAccountingRepository::new(
                self.ctx.firestore.clone(),
                user_id,
            );
            if let Err(e) = storage_repo.add_neural_cache(bytes).await {
                warn!(
                    user_id = %user_id,
                    error = %e,
                    "Failed to update storage accounting for neural cache (non-critical)"
                );
            }
        } else {
            debug!(
                user_id = %user_id,
                video_id = %video_id,
                scene_id = scene_id,
                frames = analysis.frames.len(),
                "Using existing cached neural analysis"
            );
        }

        Ok(analysis)
    }

    /// Determine the highest detection tier required by a set of styles.
    ///
    /// This is used to run detection at the highest tier needed,
    /// so all styles can use the cached results.
    pub fn highest_required_tier(styles: &[vclip_models::Style]) -> DetectionTier {
        styles
            .iter()
            .filter(|s| s.can_use_cached_analysis())
            .map(|s| s.detection_tier())
            .max_by_key(|t| t.speed_rank())
            .unwrap_or(DetectionTier::None)
    }

    /// Check if any style in the set can benefit from cached analysis.
    pub fn any_style_uses_cache(styles: &[vclip_models::Style]) -> bool {
        styles.iter().any(|s| s.can_use_cached_analysis())
    }

    /// Determine if split layout is appropriate for this scene.
    ///
    /// Split is appropriate when:
    /// - At least 2 distinct face tracks are detected
    /// - Faces appear simultaneously for at least 3 seconds
    ///
    /// When split is NOT appropriate, the split style would fall back to
    /// full-frame rendering, producing identical output to the non-split style.
    pub fn is_split_appropriate(analysis: &SceneNeuralAnalysis) -> bool {
        const MIN_SIMULTANEOUS_SECONDS: f64 = 3.0;

        if analysis.frames.is_empty() {
            return false;
        }

        // Estimate sample interval from frame count and typical duration
        let duration = analysis
            .frames
            .last()
            .map(|f| f.time)
            .unwrap_or(0.0)
            .max(1.0);
        let sample_interval = duration / analysis.frames.len().max(1) as f64;

        let mut simultaneous_time = 0.0;
        let mut distinct_tracks = std::collections::HashSet::new();

        for frame in &analysis.frames {
            if frame.faces.len() >= 2 {
                simultaneous_time += sample_interval;
            }
            for face in &frame.faces {
                if let Some(track_id) = face.track_id {
                    distinct_tracks.insert(track_id);
                }
            }
        }

        let should_split =
            distinct_tracks.len() >= 2 && simultaneous_time >= MIN_SIMULTANEOUS_SECONDS;

        tracing::debug!(
            distinct_tracks = distinct_tracks.len(),
            simultaneous_time = simultaneous_time,
            should_split = should_split,
            "Split appropriateness check"
        );

        should_split
    }
}

/// Run detection pipeline and convert results to SceneNeuralAnalysis.
async fn run_detection(
    video_path: &Path,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
    start_time: f64,
    end_time: f64,
    detection_tier: DetectionTier,
) -> WorkerResult<SceneNeuralAnalysis> {
    use vclip_media::detection::pipeline_builder::PipelineBuilder;
    use vclip_models::{BoundingBox, FaceDetection};

    info!(
        video_id = %video_id,
        scene_id = scene_id,
        start = start_time,
        end = end_time,
        tier = %detection_tier,
        "Running detection for scene analysis cache"
    );

    let mut analysis = SceneNeuralAnalysis::new(video_id, scene_id)
        .with_user(user_id)
        .with_detection_tier(detection_tier);

    let pipeline = PipelineBuilder::for_tier(detection_tier)
        .build()
        .map_err(|e| {
            WorkerError::job_failed(format!("Failed to build detection pipeline: {}", e))
        })?;

    let (_video_width, _video_height) = match pipeline.analyze(video_path, start_time, end_time).await {
        Ok(result) => {
            let video_width = result.width as f32;
            let video_height = result.height as f32;

            for frame_result in result.frames {
                let mut frame = FrameAnalysis::new(frame_result.time);

                for det in frame_result.faces.iter() {
                    let bbox = BoundingBox::from_pixels(
                        det.bbox.x as f32,
                        det.bbox.y as f32,
                        det.bbox.width as f32,
                        det.bbox.height as f32,
                        video_width,
                        video_height,
                    );
                    let mut face_det = FaceDetection::new(bbox, det.score as f32);
                    face_det = face_det.with_track_id(det.track_id);

                    if let Some(mouth) = det.mouth_openness {
                        face_det = face_det.with_mouth_openness(mouth as f32);
                    }

                    frame.add_face(face_det);
                }

                analysis.add_frame(frame);
            }

            info!(
                video_id = %video_id,
                scene_id = scene_id,
                frames = analysis.frames.len(),
                tier = ?result.tier_used,
                "Detection complete for scene analysis"
            );
            
            (video_width as u32, video_height as u32)
        }
        Err(e) => {
            warn!(
                video_id = %video_id,
                scene_id = scene_id,
                error = %e,
                "Detection failed, creating empty analysis"
            );
            // Create minimal analysis with placeholder frames
            let duration = end_time - start_time;
            let sample_interval = 0.5; // 2 FPS
            let num_samples = ((duration / sample_interval).ceil() as usize).max(1);

            for i in 0..num_samples {
                let time = start_time + (i as f64 * sample_interval);
                let frame = FrameAnalysis::new(time);
                analysis.add_frame(frame);
            }
            
            (1920, 1080) // Default dimensions for fallback
        }
    };

    // For Cinematic tier, also compute and cache shot boundaries (object detection off by default)
    if detection_tier == DetectionTier::Cinematic {
        info!(
            video_id = %video_id,
            scene_id = scene_id,
            "Computing cinematic signals (shot boundaries)..."
        );

        // Use the cinematic_signals module with object detection OFF by default
        // Object detection is enabled only when explicitly requested via job options
        let options = CinematicSignalOptions::default();
        
        let cinematic_signals = compute_cinematic_signals(
            video_path,
            start_time,
            end_time,
            options,
        )
        .await;

        match cinematic_signals {
            Ok(signals) => {
                info!(
                    video_id = %video_id,
                    scene_id = scene_id,
                    shots = signals.shots.len(),
                    has_objects = signals.object_detections.is_some(),
                    "Cinematic signals computed and added to cache"
                );
                analysis.cinematic_signals = Some(signals);
            }
            Err(e) => {
                warn!(
                    video_id = %video_id,
                    scene_id = scene_id,
                    error = %e,
                    "Failed to compute cinematic signals (non-fatal)"
                );
            }
        }
    }

    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::{FaceDetection, Style};

    #[test]
    fn test_highest_required_tier() {
        // No styles -> None tier
        assert_eq!(
            SceneAnalysisService::highest_required_tier(&[]),
            DetectionTier::None
        );

        // Basic styles only
        assert_eq!(
            SceneAnalysisService::highest_required_tier(&[Style::Intelligent]),
            DetectionTier::Basic
        );

        // Mixed styles - should return highest
        assert_eq!(
            SceneAnalysisService::highest_required_tier(&[
                Style::Intelligent,
                Style::IntelligentSpeaker,
            ]),
            DetectionTier::SpeakerAware
        );

        // Non-intelligent styles
        assert_eq!(
            SceneAnalysisService::highest_required_tier(&[Style::Split, Style::Original]),
            DetectionTier::None
        );
    }

    #[test]
    fn test_any_style_uses_cache() {
        assert!(!SceneAnalysisService::any_style_uses_cache(&[]));
        assert!(!SceneAnalysisService::any_style_uses_cache(&[Style::Split]));
        assert!(SceneAnalysisService::any_style_uses_cache(&[
            Style::Intelligent
        ]));
        assert!(SceneAnalysisService::any_style_uses_cache(&[
            Style::Split,
            Style::IntelligentSpeaker
        ]));
    }

    fn create_test_face(track_id: u32) -> FaceDetection {
        use vclip_models::BoundingBox;
        FaceDetection::new(
            BoundingBox::from_pixels(100.0, 100.0, 50.0, 50.0, 1920.0, 1080.0),
            0.9,
        )
        .with_track_id(track_id)
    }

    #[test]
    fn test_is_split_appropriate_empty_analysis() {
        let analysis = SceneNeuralAnalysis::new("test", 1);
        assert!(!SceneAnalysisService::is_split_appropriate(&analysis));
    }

    #[test]
    fn test_is_split_appropriate_single_face() {
        let mut analysis = SceneNeuralAnalysis::new("test", 1);
        for i in 0..10 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.5);
            frame.add_face(create_test_face(1));
            analysis.add_frame(frame);
        }
        // Single face track - should NOT split
        assert!(!SceneAnalysisService::is_split_appropriate(&analysis));
    }

    #[test]
    fn test_is_split_appropriate_alternating_faces() {
        let mut analysis = SceneNeuralAnalysis::new("test", 1);
        for i in 0..10 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.5);
            // Alternating faces - never simultaneous
            frame.add_face(create_test_face(if i % 2 == 0 { 1 } else { 2 }));
            analysis.add_frame(frame);
        }
        // Alternating faces - should NOT split (no simultaneous presence)
        assert!(!SceneAnalysisService::is_split_appropriate(&analysis));
    }

    #[test]
    fn test_is_split_appropriate_simultaneous_faces() {
        let mut analysis = SceneNeuralAnalysis::new("test", 1);
        // 10 frames at 0.5s intervals = 5 seconds of simultaneous faces
        for i in 0..10 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.5);
            frame.add_face(create_test_face(1));
            frame.add_face(create_test_face(2));
            analysis.add_frame(frame);
        }
        // Two faces simultaneously for 5 seconds - SHOULD split
        assert!(SceneAnalysisService::is_split_appropriate(&analysis));
    }

    #[test]
    fn test_is_split_appropriate_brief_simultaneous() {
        let mut analysis = SceneNeuralAnalysis::new("test", 1);
        // Only 2 seconds of simultaneous faces (below 3s threshold)
        for i in 0..4 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.5);
            frame.add_face(create_test_face(1));
            frame.add_face(create_test_face(2));
            analysis.add_frame(frame);
        }
        // Brief simultaneous presence - should NOT split
        assert!(!SceneAnalysisService::is_split_appropriate(&analysis));
    }
}
