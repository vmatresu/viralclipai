//! Neural analysis job processing.
//!
//! Handles background computation and caching of neural analysis results
//! (face detection, tracking) for video scenes. This pre-computes expensive
//! ML inference so that subsequent render jobs can use cached results.

use std::path::PathBuf;
use tracing::{debug, info, warn};

use vclip_media::download_video;
use vclip_models::{DetectionTier, FrameAnalysis, SceneNeuralAnalysis};
use vclip_queue::NeuralAnalysisJob;

use crate::error::{WorkerError, WorkerResult};
use crate::logging::JobLogger;
use crate::processor::EnhancedProcessingContext;

/// Process a neural analysis job.
///
/// Downloads the source video (or uses cached), runs face detection on the
/// specified scene, and stores results to R2 cache.
pub async fn process_neural_analysis_job(
    ctx: &EnhancedProcessingContext,
    job: &NeuralAnalysisJob,
) -> WorkerResult<()> {
    let logger = JobLogger::new(&job.job_id, "neural_analysis");
    logger.log_start(&format!(
        "Computing neural analysis for scene {} of video {}",
        job.scene_id, job.video_id
    ));

    // Use the neural cache service's get_or_compute to handle locking
    let result = ctx
        .neural_cache
        .get_or_compute(
            &job.user_id,
            job.video_id.as_str(),
            job.scene_id,
            job.detection_tier,
            || compute_neural_analysis(ctx, job),
        )
        .await;

    // Handle result with cinematic analysis status updates
    let (analysis, stored_bytes) = match result {
        Ok((analysis, stored_bytes)) => {
            // Mark cinematic analysis as complete for Cinematic tier
            if job.detection_tier == DetectionTier::Cinematic {
                if let Err(e) = crate::cinematic_analysis::mark_analysis_complete(
                    ctx,
                    job.video_id.as_str(),
                    job.scene_id,
                )
                .await
                {
                    warn!(
                        video_id = %job.video_id,
                        scene_id = job.scene_id,
                        error = %e,
                        "Failed to mark cinematic analysis as complete (non-critical)"
                    );
                } else {
                    info!(
                        video_id = %job.video_id,
                        scene_id = job.scene_id,
                        "Marked cinematic analysis as complete"
                    );
                }
            }
            (analysis, stored_bytes)
        }
        Err(e) => {
            // Mark cinematic analysis as failed for Cinematic tier
            if job.detection_tier == DetectionTier::Cinematic {
                crate::cinematic_analysis::mark_analysis_failed(
                    ctx,
                    job.video_id.as_str(),
                    job.scene_id,
                    &format!("{}", e),
                )
                .await
                .ok();
            }
            return Err(e);
        }
    };

    // Phase 5: Track neural cache storage using ACTUAL compressed size (non-billable)
    // Only update accounting if we actually stored new data (not a cache hit)
    if let Some(actual_bytes) = stored_bytes {
        let storage_repo = vclip_firestore::StorageAccountingRepository::new(
            ctx.firestore.clone(),
            &job.user_id,
        );
        if let Err(e) = storage_repo.add_neural_cache(actual_bytes).await {
            warn!(
                user_id = %job.user_id,
                actual_bytes = actual_bytes,
                error = %e,
                "Failed to update storage accounting for neural cache (non-critical)"
            );
        }
    }

    info!(
        video_id = %job.video_id,
        scene_id = job.scene_id,
        frames = analysis.frames.len(),
        stored_bytes = ?stored_bytes,
        "Neural analysis complete"
    );

    logger.log_completion(&format!(
        "Cached neural analysis for scene {} with {} frames",
        job.scene_id,
        analysis.frames.len()
    ));

    Ok(())
}

/// Compute neural analysis for a scene.
///
/// This is called by the cache service when there's a cache miss.
async fn compute_neural_analysis(
    ctx: &EnhancedProcessingContext,
    job: &NeuralAnalysisJob,
) -> WorkerResult<SceneNeuralAnalysis> {
    // Create work directory
    let work_dir = PathBuf::from(&ctx.config.work_dir)
        .join("neural")
        .join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;

    // Get source video path
    let video_file = download_source_for_analysis(ctx, job, &work_dir).await?;

    // Get scene timestamps from highlights
    let timestamps = get_scene_timestamps(ctx, job).await?;

    // Run face detection on the scene segment
    let analysis = run_face_detection_for_scene(
        ctx,
        &video_file,
        &job.user_id,
        job.video_id.as_str(),
        job.scene_id,
        job.detection_tier,
        &timestamps.start,
        &timestamps.end,
    )
    .await?;

    // Cleanup work directory
    if let Err(e) = tokio::fs::remove_dir_all(&work_dir).await {
        warn!("Failed to cleanup neural work directory: {}", e);
    }

    Ok(analysis)
}

/// Download source video for analysis.
async fn download_source_for_analysis(
    ctx: &EnhancedProcessingContext,
    job: &NeuralAnalysisJob,
    work_dir: &PathBuf,
) -> WorkerResult<PathBuf> {
    let video_file = work_dir.join("source.mp4");

    // Fast path: already downloaded
    if video_file.exists() {
        return Ok(video_file);
    }

    // Try source hint first (R2 cached source)
    if let Some(ref r2_key) = job.source_hint_r2_key {
        debug!(r2_key = %r2_key, "Trying source hint for neural analysis");
        if ctx.storage.download_file(r2_key, &video_file).await.is_ok() {
            return Ok(video_file);
        }
    }

    // Check Firestore for cached source
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    if let Ok(Some(video_meta)) = video_repo.get(&job.video_id).await {
        if let (Some(status), Some(r2_key)) = (
            video_meta.source_video_status,
            video_meta.source_video_r2_key.as_ref(),
        ) {
            if status == vclip_models::SourceVideoStatus::Ready {
                // Check expiration
                let expired = video_meta
                    .source_video_expires_at
                    .map(|exp| exp < chrono::Utc::now())
                    .unwrap_or(false);

                if !expired {
                    debug!(r2_key = %r2_key, "Downloading from cached source");
                    if ctx.storage.download_file(r2_key, &video_file).await.is_ok() {
                        return Ok(video_file);
                    }
                }
            }
        }
    }

    // Fallback: download from original URL via highlights
    let highlights_repo =
        vclip_firestore::HighlightsRepository::new(ctx.firestore.clone(), &job.user_id);
    let video_highlights = highlights_repo
        .get(&job.video_id)
        .await
        .map_err(|e| WorkerError::Firestore(e))?
        .ok_or_else(|| WorkerError::job_failed("Highlights not found for neural analysis"))?;

    let video_url = video_highlights
        .video_url
        .ok_or_else(|| WorkerError::job_failed("No video URL in highlights"))?;

    info!(
        video_id = %job.video_id,
        url = %video_url,
        "Downloading video from origin for neural analysis"
    );

    download_video(&video_url, &video_file).await?;

    Ok(video_file)
}

/// Scene timestamp data.
struct SceneTimestamps {
    start: String,
    end: String,
}

/// Get scene timestamps from highlights.
async fn get_scene_timestamps(
    ctx: &EnhancedProcessingContext,
    job: &NeuralAnalysisJob,
) -> WorkerResult<SceneTimestamps> {
    let highlights_repo =
        vclip_firestore::HighlightsRepository::new(ctx.firestore.clone(), &job.user_id);
    let video_highlights = highlights_repo
        .get(&job.video_id)
        .await
        .map_err(|e| WorkerError::Firestore(e))?
        .ok_or_else(|| WorkerError::job_failed("Highlights not found"))?;

    let scene = video_highlights
        .highlights
        .iter()
        .find(|h| h.id == job.scene_id)
        .ok_or_else(|| {
            WorkerError::job_failed(format!("Scene {} not found in highlights", job.scene_id))
        })?;

    Ok(SceneTimestamps {
        start: scene.start.clone(),
        end: scene.end.clone(),
    })
}

/// Run face detection on a video segment using the detection pipeline.
async fn run_face_detection_for_scene(
    _ctx: &EnhancedProcessingContext,
    video_path: &PathBuf,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
    detection_tier: DetectionTier,
    start: &str,
    end: &str,
) -> WorkerResult<SceneNeuralAnalysis> {
    use vclip_media::detection::pipeline_builder::PipelineBuilder;
    use vclip_media::intelligent::parse_timestamp;
    use vclip_models::{BoundingBox, FaceDetection};

    let start_secs = parse_timestamp(start).unwrap_or(0.0);
    let end_secs = parse_timestamp(end).unwrap_or(30.0);

    info!(
        video_id = %video_id,
        scene_id = scene_id,
        start = start_secs,
        end = end_secs,
        "Running face detection for neural cache"
    );

    let mut analysis = SceneNeuralAnalysis::new(video_id, scene_id)
        .with_user(user_id)
        .with_detection_tier(detection_tier);

    let pipeline = PipelineBuilder::for_tier(detection_tier)
        .build()
        .map_err(|e| WorkerError::job_failed(format!("Failed to build detection pipeline: {}", e)))?;

    // Run detection
    match pipeline.analyze(video_path, start_secs, end_secs).await {
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

                    // Add tracking ID if available
                    face_det = face_det.with_track_id(det.track_id);

                    // Add mouth openness if available
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
                "Detection complete for neural cache"
            );
        }
        Err(e) => {
            warn!(
                video_id = %video_id,
                scene_id = scene_id,
                error = %e,
                "Detection failed, creating empty analysis"
            );
            // Create minimal analysis with placeholder frames
            let duration = end_secs - start_secs;
            let sample_interval = 0.5; // 2 FPS
            let num_samples = ((duration / sample_interval).ceil() as usize).max(1);

            for i in 0..num_samples {
                let time = start_secs + (i as f64 * sample_interval);
                let frame = FrameAnalysis::new(time);
                analysis.add_frame(frame);
            }
        }
    }

    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::NEURAL_ANALYSIS_VERSION;

    #[test]
    fn test_scene_analysis_creation() {
        let analysis = SceneNeuralAnalysis::new("video123", 1).with_user("user123");
        assert_eq!(analysis.scene_id, 1);
        assert_eq!(analysis.video_id, "video123");
        assert_eq!(analysis.user_id, Some("user123".to_string()));
        assert_eq!(analysis.analysis_version, NEURAL_ANALYSIS_VERSION);
    }
}
