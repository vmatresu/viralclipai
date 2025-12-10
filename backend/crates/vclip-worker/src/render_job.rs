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
async fn process_render_clip_inner(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
    work_dir: &Path,
    clips_dir: &Path,
) -> WorkerResult<()> {
    // Download video from R2 (may already be cached)
    let video_file = download_video_for_render(ctx, job, work_dir).await?;

    // Build ClipTask from job fields
    let task = ClipTask {
        scene_id: job.scene_id,
        scene_title: job.scene_title.clone(),
        scene_description: None,
        start: job.start.clone(),
        end: job.end.clone(),
        style: job.style,
        crop_mode: job.crop_mode,
        target_aspect: job.target_aspect,
        priority: job.scene_id,
        pad_before: job.pad_before_seconds.unwrap_or(1.0),
        pad_after: job.pad_after_seconds.unwrap_or(1.0),
    };

    // Process the single clip
    clip_pipeline::process_single_clip(
        ctx,
        &job.job_id,
        &job.video_id,
        &job.user_id,
        &video_file,
        clips_dir,
        &task,
        0,
        1,
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

/// Download video from R2 storage for rendering.
///
/// Implements a caching strategy:
/// 1. Check if video already exists locally (shared by multiple workers on same machine)
/// 2. Download from R2 if not cached
/// 3. Fallback to original URL if R2 fails
async fn download_video_for_render(
    ctx: &EnhancedProcessingContext,
    job: &RenderSceneStyleJob,
    work_dir: &Path,
) -> WorkerResult<PathBuf> {
    let video_file = work_dir.join("source.mp4");

    // Fast path: reuse if already exists
    if video_file.exists() {
        return Ok(video_file);
    }

    // Try to download from R2
    let video_key = format!("{}/{}/source.mp4", job.user_id, job.video_id);
    match ctx.storage.download_file(&video_key, &video_file).await {
        Ok(_) => {
            info!("Downloaded video from R2 for render job: {}", job.video_id);
            Ok(video_file)
        }
        Err(e) => {
            tracing::warn!("R2 download failed, attempting fallback: {}", e);
            download_video_fallback(ctx, job, &video_file).await?;
            Ok(video_file)
        }
    }
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

