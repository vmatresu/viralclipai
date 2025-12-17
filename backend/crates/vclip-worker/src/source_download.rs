//! Source video download and caching service.
//!
//! This module handles downloading source videos with coordinated caching:
//! - Checks local work directory first
//! - Uses R2 cache when available
//! - Coordinates with other workers via Redis to prevent duplicate downloads
//! - Falls back to original URL when necessary
//!
//! # Architecture
//!
//! The download coordinator ensures only one worker downloads a source video
//! at a time, with others waiting for the result. This prevents redundant
//! downloads and reduces bandwidth/storage costs.

use std::path::{Path, PathBuf};

use tracing::info;

use vclip_media::download_video;
use vclip_models::VideoHighlights;
use vclip_queue::ReprocessScenesJob;

use crate::download_coordinator::{DownloadAction, SourceVideoDownloadCoordinator, WaitResult};
use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;

/// Download source video from R2 (cached) or original URL (fallback).
///
/// Uses `SourceVideoDownloadCoordinator` to prevent duplicate downloads:
/// 1. Check if source already exists in local work directory
/// 2. Use coordinator to check cache or detect in-progress download
/// 3. If another worker is downloading, wait for completion
/// 4. If cache available, download from R2
/// 5. Fall back to original video URL with lock held
pub async fn download_source_video(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    work_dir: &PathBuf,
    highlights: &VideoHighlights,
) -> WorkerResult<PathBuf> {
    let video_file = work_dir.join("source.mp4");

    // Check if source already exists in local work directory (from previous/concurrent job)
    if let Some(existing) = check_local_source(&video_file, job).await {
        return Ok(existing);
    }

    ctx.progress
        .log(&job.job_id, "Checking source video status...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    // Use coordinator to handle download coordination
    let coordinator = SourceVideoDownloadCoordinator::new(ctx.redis.clone(), ctx.firestore.clone());

    let action = coordinator
        .acquire_or_wait_for_download(&job.user_id, job.video_id.as_str())
        .await?;

    match action {
        DownloadAction::UseCache { r2_key } => {
            if let Ok(path) = download_from_cache(ctx, job, &video_file, &r2_key).await {
                return Ok(path);
            }
        }

        DownloadAction::WaitForOther => {
            if let Some(path) = wait_for_other_download(ctx, job, &video_file, &coordinator).await?
            {
                return Ok(path);
            }
        }

        DownloadAction::PerformDownload { lock_token } => {
            if let Some(path) = perform_coordinated_download(
                ctx,
                job,
                &video_file,
                highlights,
                &coordinator,
                &lock_token,
            )
            .await?
            {
                return Ok(path);
            }
        }
    }

    // Try legacy R2 location for backwards compatibility
    if let Some(path) = try_legacy_r2_location(ctx, job, &video_file).await {
        return Ok(path);
    }

    // Final fallback: try original URL directly
    download_from_original_url(ctx, job, &video_file, highlights).await
}

/// Check if source already exists in local work directory.
async fn check_local_source(video_file: &PathBuf, job: &ReprocessScenesJob) -> Option<PathBuf> {
    if video_file.exists() {
        if let Ok(metadata) = tokio::fs::metadata(video_file).await {
            if metadata.len() > 0 {
                info!(
                    video_id = %job.video_id,
                    path = ?video_file,
                    size_mb = metadata.len() as f64 / 1_048_576.0,
                    "Using existing source video from local work directory"
                );
                return Some(video_file.clone());
            }
        }
    }
    None
}

/// Download source from R2 cache.
async fn download_from_cache(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &PathBuf,
    r2_key: &str,
) -> WorkerResult<PathBuf> {
    ctx.progress
        .log(&job.job_id, "Downloading from cache...")
        .await
        .ok();

    match ctx.storage.download_file(r2_key, video_file).await {
        Ok(_) => {
            info!(
                video_id = %job.video_id,
                r2_key = r2_key,
                "Downloaded source video from R2 cache"
            );
            Ok(video_file.clone())
        }
        Err(e) => {
            info!(
                video_id = %job.video_id,
                error = %e,
                "R2 cache download failed, falling back to original URL"
            );
            Err(WorkerError::job_failed("Cache download failed"))
        }
    }
}

/// Wait for another worker to complete the download.
async fn wait_for_other_download(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &PathBuf,
    coordinator: &SourceVideoDownloadCoordinator,
) -> WorkerResult<Option<PathBuf>> {
    ctx.progress
        .log(&job.job_id, "Waiting for background download...")
        .await
        .ok();

    let wait_result = coordinator
        .wait_for_download_complete(&job.user_id, job.video_id.as_str(), None)
        .await?;

    match wait_result {
        WaitResult::Ready { r2_key } => {
            ctx.progress
                .log(&job.job_id, "Downloading from cache...")
                .await
                .ok();

            match ctx.storage.download_file(&r2_key, video_file).await {
                Ok(_) => {
                    info!(
                        video_id = %job.video_id,
                        r2_key = r2_key.as_str(),
                        "Downloaded source video from R2 after waiting for background job"
                    );
                    return Ok(Some(video_file.clone()));
                }
                Err(e) => {
                    tracing::warn!(
                        video_id = %job.video_id,
                        error = %e,
                        "R2 download failed after wait, falling back to original URL"
                    );
                }
            }
        }
        WaitResult::Failed { error } => {
            info!(
                video_id = %job.video_id,
                error = error.as_str(),
                "Background download failed, trying original URL"
            );
        }
        WaitResult::Timeout => {
            info!(
                video_id = %job.video_id,
                "Timeout waiting for background download, trying original URL"
            );
        }
    }

    Ok(None)
}

/// Perform the download with coordination lock held.
async fn perform_coordinated_download(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &PathBuf,
    highlights: &VideoHighlights,
    coordinator: &SourceVideoDownloadCoordinator,
    lock_token: &str,
) -> WorkerResult<Option<PathBuf>> {
    ctx.progress
        .log(&job.job_id, "Downloading source video...")
        .await
        .ok();

    if let Some(ref video_url) = highlights.video_url {
        // Mark as downloading in Firestore
        coordinator
            .mark_downloading(&job.user_id, job.video_id.as_str())
            .await
            .ok();

        match download_video(video_url, video_file).await {
            Ok(_) => {
                info!(
                    video_id = %job.video_id,
                    url = video_url.as_str(),
                    "Downloaded source video from original URL"
                );

                // Upload to R2 for future requests
                upload_source_to_r2_async(ctx, job, video_file).await;

                // Release lock
                coordinator
                    .release_lock(&job.user_id, job.video_id.as_str(), lock_token)
                    .await
                    .ok();

                return Ok(Some(video_file.clone()));
            }
            Err(e) => {
                let err_msg = format!("Download failed: {}", e);
                coordinator
                    .mark_failed(&job.user_id, job.video_id.as_str(), &err_msg)
                    .await
                    .ok();
                coordinator
                    .release_lock(&job.user_id, job.video_id.as_str(), lock_token)
                    .await
                    .ok();

                ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                return Err(WorkerError::job_failed(&err_msg));
            }
        }
    } else {
        let err_msg = "No source video available: not in R2 cache and no original URL in highlights data.";
        coordinator
            .mark_failed(&job.user_id, job.video_id.as_str(), err_msg)
            .await
            .ok();

        // Release lock and return error
        coordinator
            .release_lock(&job.user_id, job.video_id.as_str(), lock_token)
            .await
            .ok();

        ctx.progress.error(&job.job_id, err_msg).await.ok();
        return Err(WorkerError::job_failed(err_msg));
    }
}

/// Try legacy R2 location for backwards compatibility.
async fn try_legacy_r2_location(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &PathBuf,
) -> Option<PathBuf> {
    let legacy_source_key = format!("{}/{}/source.mp4", job.user_id, job.video_id.as_str());

    match ctx.storage.download_file(&legacy_source_key, video_file).await {
        Ok(_) => {
            info!(
                video_id = %job.video_id,
                "Downloaded source video from legacy R2 location: {}",
                legacy_source_key
            );
            Some(video_file.clone())
        }
        Err(r2_error) => {
            info!(
                video_id = %job.video_id,
                error = %r2_error,
                "Source video not found in legacy R2 location"
            );
            None
        }
    }
}

/// Download from original URL as final fallback.
async fn download_from_original_url(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &PathBuf,
    highlights: &VideoHighlights,
) -> WorkerResult<PathBuf> {
    if let Some(ref video_url) = highlights.video_url {
        ctx.progress
            .log(&job.job_id, "Downloading original video from source URL...")
            .await
            .ok();

        match download_video(video_url, video_file).await {
            Ok(_) => {
                info!(
                    video_id = %job.video_id,
                    url = video_url.as_str(),
                    "Downloaded source video from original URL (final fallback)"
                );
                upload_source_to_r2_async(ctx, job, video_file).await;
                return Ok(video_file.clone());
            }
            Err(url_error) => {
                let err_msg = format!(
                    "Failed to download from original URL {}: {}",
                    video_url, url_error
                );
                ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                return Err(WorkerError::job_failed(&err_msg));
            }
        }
    }

    let err_msg = "No source video available: not in R2 cache and no original URL in highlights data.";
    ctx.progress.error(&job.job_id, err_msg).await.ok();
    Err(WorkerError::job_failed(err_msg))
}

/// Upload source video to R2 asynchronously (non-blocking for main job).
/// This replaces the background download job - we upload immediately since we already have the file.
pub async fn upload_source_to_r2_async(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &Path,
) {
    use chrono::{Duration as ChronoDuration, Utc};

    let r2_key = format!(
        "sources/{}/{}/source.mp4",
        job.user_id,
        job.video_id.as_str()
    );
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

    // Mark as uploading (non-critical)
    video_repo
        .set_source_video_downloading(&job.video_id)
        .await
        .ok();

    // Upload to R2
    match ctx
        .storage
        .upload_file(video_file, &r2_key, "video/mp4")
        .await
    {
        Ok(_) => {
            info!(
                video_id = %job.video_id,
                r2_key = %r2_key,
                "Uploaded source video to R2 cache"
            );

            // Calculate expiration time (24 hours)
            let expires_at = Utc::now() + ChronoDuration::hours(24);

            // Mark as ready in Firestore (non-critical)
            if let Err(e) = video_repo
                .set_source_video_ready(&job.video_id, &r2_key, expires_at)
                .await
            {
                tracing::warn!(
                    video_id = %job.video_id,
                    error = %e,
                    "Failed to update source video status in Firestore (non-critical)"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                video_id = %job.video_id,
                error = %e,
                "Failed to upload source video to R2 cache (non-critical)"
            );
            // Mark as failed (non-critical)
            video_repo
                .set_source_video_failed(&job.video_id, Some(&e.to_string()))
                .await
                .ok();
        }
    }
}
