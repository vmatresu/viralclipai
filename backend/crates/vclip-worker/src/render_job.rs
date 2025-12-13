//! Render job processing.
//!
//! Handles fine-grained render jobs (one scene, one style = one clip).
//! This module provides coordinated source video management across
//! distributed workers using RAII-style guards.

use std::path::{Path, PathBuf};
use tracing::info;

use vclip_media::download_video;
use vclip_models::ClipTask;
use vclip_queue::RenderSceneStyleJob;

use crate::clip_pipeline;
use crate::error::{WorkerError, WorkerResult};
use crate::logging::JobLogger;
use crate::processor::EnhancedProcessingContext;
use crate::raw_segment_cache::raw_segment_r2_key;
use crate::source_video_coordinator::SourceVideoCoordinator;

/// RAII guard for coordinated job lifecycle.
///
/// Ensures the job counter is always decremented when processing completes,
/// even on panic or early return. This implements the "scoped guard" pattern
/// for distributed resources.
pub struct RenderJobGuard<'a> {
    coordinator: &'a SourceVideoCoordinator,
    user_id: String,
    video_id: String,
    work_dir: PathBuf,
    finished: bool,
}

impl<'a> RenderJobGuard<'a> {
    /// Create a new job guard, registering with the coordinator.
    ///
    /// Returns the guard and the current active job count.
    pub async fn new(
        coordinator: &'a SourceVideoCoordinator,
        user_id: &str,
        video_id: &str,
        work_dir: PathBuf,
    ) -> Result<(Self, i64), WorkerError> {
        let active_count = coordinator
            .job_started(user_id, video_id)
            .await
            .map_err(|e| WorkerError::job_failed(format!("Failed to register job: {}", e)))?;

        Ok((
            Self {
                coordinator,
                user_id: user_id.to_string(),
                video_id: video_id.to_string(),
                work_dir,
                finished: false,
            },
            active_count,
        ))
    }

    /// Mark the job as finished and perform cleanup if this was the last job.
    ///
    /// This should be called explicitly for controlled cleanup. If not called,
    /// the Drop implementation will attempt cleanup with a blocking call.
    pub async fn finish(mut self) -> bool {
        self.finished = true;
        self.do_finish().await
    }

    /// Internal finish logic.
    async fn do_finish(&self) -> bool {
        let should_cleanup = self
            .coordinator
            .job_finished(&self.user_id, &self.video_id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to decrement job count: {}", e);
                false
            });

        if should_cleanup {
            info!(
                video_id = %self.video_id,
                "Last render job complete, cleaning up work directory"
            );
            if let Err(e) = SourceVideoCoordinator::cleanup_work_dir(&self.work_dir).await {
                tracing::warn!("Failed to cleanup work directory: {}", e);
            }
        }

        should_cleanup
    }
}

impl Drop for RenderJobGuard<'_> {
    fn drop(&mut self) {
        if !self.finished {
            // Emergency cleanup - job was not properly finished.
            // This can happen on panic. Log a warning as we can't do async cleanup here.
            tracing::warn!(
                video_id = %self.video_id,
                "RenderJobGuard dropped without finish() - job counter may be incorrect"
            );
        }
    }
}

/// Process a single render job (fine-grained: one scene, one style).
///
/// This is the atomic unit of work for the new job model. Each job
/// produces exactly one clip, enabling fine-grained parallelization.
pub async fn process_render_job(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
) -> WorkerResult<()> {
    let logger = JobLogger::new(&job.job_id, "render_scene_style");
    logger.log_start(&format!(
        "Rendering scene {} with style {} for video {}",
        job.scene_id, job.style, job.video_id
    ));

    // Create work directory for this video (shared across render jobs)
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Register with coordinator using RAII guard
    let (guard, active_count) = RenderJobGuard::new(
        &ctx.source_coordinator,
        &job.user_id,
        job.video_id.as_str(),
        work_dir.clone(),
    )
    .await?;

    info!(
        video_id = %job.video_id,
        active_jobs = active_count,
        "Registered render job for video"
    );

    // Process the clip
    let result = process_render_clip_inner(ctx, job, &work_dir, &clips_dir).await;

    // Finish the guard, which handles cleanup if this was the last job
    guard.finish().await;

    // Propagate any error from the actual processing
    result?;

    logger.log_completion("Render complete");
    Ok(())
}

/// Inner processing logic for render job.
///
/// Cache-first strategy (optimized):
/// 1. Check if raw segment already exists in R2 (FIRST - avoid full source download)
/// 2. If raw exists: download raw segment directly
/// 3. If raw missing: download full source, extract raw segment, upload to R2
/// 4. Apply style to the raw segment
///
/// This order ensures we avoid downloading the full source video when a cached
/// raw segment for the scene already exists.
async fn process_render_clip_inner(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
    work_dir: &Path,
    clips_dir: &Path,
) -> WorkerResult<()> {
    // Only premium tiers (SpeakerAware, MotionAware) should trigger cache generation.
    // Lower tiers can consume cache if available but never trigger expensive generation.
    if job.style.should_generate_cached_analysis() {
        let required_tier = job.style.detection_tier();
        if let Ok(None) = ctx
            .neural_cache
            .get_cached_for_tier(&job.user_id, job.video_id.as_str(), job.scene_id, required_tier)
            .await
        {
            info!(
                scene_id = job.scene_id,
                tier = %required_tier,
                "Analysis cache miss (premium tier) - triggering background computation"
            );
            trigger_neural_analysis_job(ctx, job).await;
        }
    }

    // Calculate padded timestamps for raw segment
    let pad_before = job.pad_before_seconds.unwrap_or(1.0);
    let pad_after = job.pad_after_seconds.unwrap_or(1.0);
    let start_secs = vclip_media::intelligent::parse_timestamp(&job.start).unwrap_or(0.0);
    let end_secs = vclip_media::intelligent::parse_timestamp(&job.end).unwrap_or(30.0);
    let padded_start = (start_secs - pad_before).max(0.0);
    let padded_end = end_secs + pad_after;
    let padded_start_ts = format_timestamp(padded_start);
    let padded_end_ts = format_timestamp(padded_end);

    // CACHE-FIRST: Try to get raw segment from R2 BEFORE downloading full source
    let raw_segment = work_dir.join(format!("raw_{}.mp4", job.scene_id));
    let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), job.scene_id);
    let mut raw_created = false;

    // Step 1: Check if raw segment exists locally
    if raw_segment.exists() {
        info!(
            scene_id = job.scene_id,
            "Using existing local raw segment: {:?}",
            raw_segment
        );
    }
    // Step 2: Check if raw segment exists in R2 (BEFORE downloading full source)
    else if ctx.raw_cache.check_raw_exists(&r2_key).await {
        match ctx.raw_cache.download_raw_segment(&r2_key, &raw_segment).await {
            Ok(true) => {
                info!(
                    scene_id = job.scene_id,
                    r2_key = %r2_key,
                    "Downloaded cached raw segment from R2 (skipped full source download)"
                );
            }
            Ok(false) | Err(_) => {
                // R2 check said exists but download failed - fall through to extract from source
                info!(
                    scene_id = job.scene_id,
                    "Raw segment R2 download failed, falling back to source extraction"
                );
                let source_video = download_video_for_render(ctx, job, work_dir).await?;
                let (seg, created) = ctx
                    .raw_cache
                    .get_or_create_with_outcome(
                        &job.user_id,
                        job.video_id.as_str(),
                        job.scene_id,
                        &source_video,
                        &padded_start_ts,
                        &padded_end_ts,
                        work_dir,
                    )
                    .await?;
                raw_created = created;
                // Ensure we use the correct path
                if seg != raw_segment && seg.exists() {
                    tokio::fs::copy(&seg, &raw_segment).await.ok();
                }
            }
        }
    }
    // Step 3: Raw segment not in R2 - download full source and extract
    else {
        info!(
            scene_id = job.scene_id,
            "Raw segment not cached, downloading source video..."
        );
        let source_video = download_video_for_render(ctx, job, work_dir).await?;
        let (seg, created) = ctx
            .raw_cache
            .get_or_create_with_outcome(
                &job.user_id,
                job.video_id.as_str(),
                job.scene_id,
                &source_video,
                &padded_start_ts,
                &padded_end_ts,
                work_dir,
            )
            .await?;
        raw_created = created;
        // Ensure we use the correct path
        if seg != raw_segment && seg.exists() {
            tokio::fs::copy(&seg, &raw_segment).await.ok();
        }
    }

    // Track storage accounting for newly created raw segments
    if raw_created {
        if let Ok(metadata) = tokio::fs::metadata(&raw_segment).await {
            let file_size = metadata.len();
            let storage_repo = vclip_firestore::StorageAccountingRepository::new(
                ctx.firestore.clone(),
                &job.user_id,
            );
            if let Err(e) = storage_repo.add_raw_segment(file_size).await {
                tracing::warn!(
                    user_id = %job.user_id,
                    size_bytes = file_size,
                    error = %e,
                    "Failed to update storage accounting for raw segment (non-critical)"
                );
            }
        }
    }

    info!(
        scene_id = job.scene_id,
        style = %job.style,
        raw_segment = ?raw_segment,
        "Using raw segment for styled render"
    );

    // Build ClipTask from job fields
    // Note: When using raw segment, start/end are relative to the raw segment (i.e., 0 to duration)
    // The raw segment already has padding applied, so we use 0-relative timestamps
    let task = ClipTask {
        scene_id: job.scene_id,
        scene_title: job.scene_title.clone(),
        scene_description: None,
        // When using raw segment, timestamps are relative to segment start
        start: "00:00:00".to_string(),
        end: format_timestamp(padded_end - padded_start),
        style: job.style,
        crop_mode: job.crop_mode,
        target_aspect: job.target_aspect,
        priority: job.scene_id,
        // Padding already applied in raw extraction, so set to 0
        pad_before: 0.0,
        pad_after: 0.0,
    };

    // Step 3: Process the clip using the raw segment as input
    // Phase 4 fix: Pass raw_r2_key for atomic clip creation (avoids consistency gap)
    let raw_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), job.scene_id);
    clip_pipeline::process_single_clip_with_raw_key(
        ctx,
        &job.job_id,
        &job.video_id,
        &job.user_id,
        &raw_segment, // Use raw segment instead of full source
        clips_dir,
        &task,
        0,
        1,
        Some(raw_key), // Set raw_r2_key atomically during clip creation
    )
    .await?;

    // Increment completed clips count in Firestore
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    if let Err(e) = video_repo.increment_completed_clips(&job.video_id).await {
        tracing::warn!(
            "Failed to increment completed clips for video {}: {}",
            job.video_id,
            e
        );
    }

    Ok(())
}

/// Format seconds as HH:MM:SS.mmm timestamp for FFmpeg.
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}

/// Download video from R2 storage for rendering.
///
/// Implements a caching strategy (Phase 2 compliant):
/// 1. Check if video already exists locally (shared by multiple workers on same machine)
/// 2. Check Firestore for cached source video status (new sources/{user}/{video}/source.mp4 key)
/// 3. Try legacy R2 location for backwards compatibility
/// 4. Fallback to original URL if R2 fails
async fn download_video_for_render(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
    work_dir: &Path,
) -> WorkerResult<PathBuf> {
    let video_file = work_dir.join("source.mp4");

    // Fast path: reuse if already exists locally
    if video_file.exists() {
        return Ok(video_file);
    }

    // Phase 2: Check Firestore for cached source video status first
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    if let Ok(Some(video_meta)) = video_repo.get(&job.video_id).await {
        if let (Some(status), Some(ref r2_key), Some(expires_at)) = (
            video_meta.source_video_status,
            &video_meta.source_video_r2_key,
            video_meta.source_video_expires_at,
        ) {
            if status == vclip_models::SourceVideoStatus::Ready
                && expires_at > chrono::Utc::now()
            {
                info!(
                    video_id = %job.video_id,
                    r2_key = %r2_key,
                    expires_at = %expires_at,
                    "Using cached source video from Firestore metadata"
                );
                match ctx.storage.download_file(r2_key, &video_file).await {
                    Ok(_) => {
                        info!("Downloaded video from cached R2 key: {}", r2_key);
                        return Ok(video_file);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to download from cached R2 key {}: {}, trying fallbacks",
                            r2_key, e
                        );
                    }
                }
            } else if status == vclip_models::SourceVideoStatus::Ready {
                // Expired - mark as expired
                info!("Cached source video expired at {}, falling back", expires_at);
                video_repo.set_source_video_expired(&job.video_id).await.ok();
            }
        }
    }

    // Try legacy R2 location for backwards compatibility
    let legacy_key = format!("{}/{}/source.mp4", job.user_id, job.video_id);
    match ctx.storage.download_file(&legacy_key, &video_file).await {
        Ok(_) => {
            info!("Downloaded video from legacy R2 location: {}", legacy_key);
            return Ok(video_file);
        }
        Err(e) => {
            tracing::warn!("Legacy R2 download failed: {}, attempting URL fallback", e);
        }
    }

    // Final fallback: download from original URL
    download_video_fallback(ctx, job, &video_file).await?;
    Ok(video_file)
}

/// Fallback: download from original video URL stored in Firestore highlights.
async fn download_video_fallback(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
    video_file: &Path,
) -> WorkerResult<()> {
    // Load highlights from Firestore (source of truth)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
    );

    let video_highlights = highlights_repo
        .get(&job.video_id)
        .await
        .map_err(|e| WorkerError::Firestore(e))?
        .ok_or_else(|| WorkerError::job_failed("Highlights not found in Firestore"))?;

    let video_url = video_highlights.video_url.ok_or_else(|| {
        WorkerError::job_failed("No video URL in highlights for fallback download")
    })?;

    download_video(&video_url, video_file).await?;
    Ok(())
}

/// Trigger a neural analysis job for a scene (fire-and-forget).
///
/// This is called when a render job needs neural analysis but the cache is empty.
/// The neural analysis will be computed in the background and cached for future use.
async fn trigger_neural_analysis_job(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
) {
    // Use shared queue client from context (avoids hot-path from_env() calls)
    let Some(ref queue) = ctx.job_queue else {
        tracing::debug!("No shared queue client available, skipping neural analysis job");
        return;
    };

    // Check Firestore for source video R2 key hint
    let source_hint = match vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id)
        .get(&job.video_id)
        .await
    {
        Ok(Some(video_meta)) => video_meta.source_video_r2_key,
        _ => None,
    };

    // Use style's detection tier (not hardcoded SpeakerAware)
    let detection_tier = job.style.detection_tier();
    
    let neural_job = vclip_queue::NeuralAnalysisJob {
        job_id: vclip_models::JobId::new(),
        user_id: job.user_id.clone(),
        video_id: job.video_id.clone(),
        scene_id: job.scene_id,
        source_hint_r2_key: source_hint,
        detection_tier,
        created_at: chrono::Utc::now(),
    };

    // Try to enqueue - failures are not critical (render will still work, just slower)
    match queue.enqueue_neural_analysis(neural_job).await {
        Ok(msg_id) => {
            info!(
                video_id = %job.video_id,
                scene_id = job.scene_id,
                tier = ?detection_tier,
                message_id = %msg_id,
                "Enqueued neural analysis job for future cache"
            );
        }
        Err(e) => {
            // Duplicate or other error - not critical
            tracing::debug!(
                video_id = %job.video_id,
                scene_id = job.scene_id,
                error = %e,
                "Failed to enqueue neural analysis job (non-critical)"
            );
        }
    }
}
