//! Scene reprocessing logic.
//!
//! This module handles the reprocessing of specific scenes from a previously
//! processed video, allowing users to apply different styles without re-running
//! the AI analysis.
//!
//! # Optimized Processing Strategy
//!
//! The pipeline now implements parallel processing to minimize latency:
//! 1. Check which scenes have cached raw segments (local or R2)
//! 2. For uncached scenes, try yt-dlp segment download first (avoids full source download)
//! 3. Process cached scenes immediately while downloading full source for remaining scenes
//! 4. Process remaining scenes once source is available

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{info, warn};

use vclip_media::download_video;
use vclip_models::{ClipStatus, ClipTask, Highlight};
use vclip_queue::ReprocessScenesJob;

use crate::clip_pipeline;
use crate::error::{WorkerError, WorkerResult};
use crate::processor::{EnhancedProcessingContext, JobLogger};

/// Process a reprocess scenes job.
///
/// This is different from `process_video_job` as it:
/// 1. Loads existing highlights from Firestore (doesn't re-run AI analysis)
/// 2. Filters to only the requested scene IDs
/// 3. **Optimized**: Processes cached scenes in parallel while downloading uncached ones
/// 4. Tries yt-dlp segment download before falling back to full source download
pub async fn reprocess_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
) -> WorkerResult<()> {
    let job_logger = JobLogger::new(&job.job_id, "reprocess_scenes");
    job_logger.log_start("Starting scene processing job");

    ctx.progress
        .log(&job.job_id, "Loading video data...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 5).await.ok();

    // Load existing highlights from Firestore (source of truth)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
    );

    let video_highlights = highlights_repo
        .get(&job.video_id)
        .await
        .map_err(|e| WorkerError::Firestore(e))?
        .ok_or_else(|| WorkerError::job_failed("Highlights not found in Firestore"))?;

    // Filter highlights to only requested scene IDs
    let scene_ids_set: std::collections::HashSet<_> = job.scene_ids.iter().copied().collect();
    let selected_highlights: Vec<_> = video_highlights
        .highlights
        .iter()
        .filter(|h| scene_ids_set.contains(&h.id))
        .cloned()
        .collect();

    if selected_highlights.is_empty() {
        return Err(WorkerError::job_failed("No valid scenes found"));
    }

    // Calculate total clips
    let total_clips = selected_highlights.len() * job.styles.len();
    ctx.progress
        .log(
            &job.job_id,
            format!(
                "Processing {} scenes with {} styles ({} total clips)",
                selected_highlights.len(),
                job.styles.len(),
                total_clips
            ),
        )
        .await
        .ok();

    // Create work directory
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;
    
    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Partition scenes into cached (local/R2) and uncached
    let (cached_scenes, uncached_scenes) = partition_scenes_by_cache_status(
        ctx, job, &selected_highlights, &work_dir
    ).await;
    
    info!(
        video_id = %job.video_id,
        cached_count = cached_scenes.len(),
        uncached_count = uncached_scenes.len(),
        "Partitioned scenes by cache status"
    );

    // Generate clip tasks from selected highlights
    let selected_refs: Vec<&Highlight> = selected_highlights.iter().collect();
    let clip_tasks = clip_pipeline::tasks::generate_clip_tasks_from_firestore_highlights_with_params(
        &selected_refs,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
        job.streamer_split_params.clone(),
    );

    ctx.progress
        .log(&job.job_id, format!("Generating {} clips...", total_clips))
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    // OPTIMIZED PROCESSING STRATEGY:
    // 1. Process cached scenes first (they're ready immediately)
    // 2. For uncached scenes, try yt-dlp segment download first
    // 3. For scenes where yt-dlp fails, download full source and extract
    
    let video_url = video_highlights.video_url.clone();
    let final_completed: u32;
    
    // If all scenes are cached, process them directly
    if uncached_scenes.is_empty() {
        info!(
            video_id = %job.video_id,
            "All scenes cached, processing directly without source download"
        );
        
        final_completed = process_scenes_with_cache(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &cached_scenes,
            &video_highlights,
            total_clips,
        )
        .await?;
    }
    // If all scenes are uncached, try yt-dlp then fall back to full source
    else if cached_scenes.is_empty() {
        final_completed = process_uncached_scenes_optimized(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &uncached_scenes,
            &video_highlights,
            video_url.as_deref(),
            total_clips,
        )
        .await?;
    }
    // Mixed: process cached scenes first, then uncached scenes
    else {
        info!(
            video_id = %job.video_id,
            "Processing cached scenes first, then acquiring uncached scenes"
        );
        
        // Process cached scenes first (they're ready immediately)
        let cached_completed = process_scenes_with_cache(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &cached_scenes,
            &video_highlights,
            total_clips,
        )
        .await?;
        
        // Then acquire and process uncached scenes (tries yt-dlp first)
        let uncached_completed = process_uncached_scenes_optimized(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &uncached_scenes,
            &video_highlights,
            video_url.as_deref(),
            total_clips,
        )
        .await?;
        
        final_completed = cached_completed + uncached_completed;
    }

    // Update video metadata - add new clips to existing count
    update_video_clip_count(ctx, job, final_completed).await?;

    // Cleanup work directory
    if work_dir.exists() {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    ctx.progress.progress(&job.job_id, 100).await.ok();
    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    job_logger.log_completion(&format!(
        "Reprocessed {} scenes, {} clips completed",
        selected_highlights.len(),
        final_completed
    ));

    Ok(())
}

/// Partition scenes into cached (available locally or in R2) and uncached.
async fn partition_scenes_by_cache_status(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    highlights: &[Highlight],
    work_dir: &PathBuf,
) -> (Vec<Highlight>, Vec<Highlight>) {
    use crate::raw_segment_cache::raw_segment_r2_key;
    
    let mut cached = Vec::new();
    let mut uncached = Vec::new();
    
    for highlight in highlights {
        let scene_id = highlight.id;
        
        // Check local file first (fast path)
        let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));
        if local_path.exists() {
            cached.push(highlight.clone());
            continue;
        }
        
        // Check R2
        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
        if ctx.raw_cache.check_raw_exists(&r2_key).await {
            cached.push(highlight.clone());
        } else {
            uncached.push(highlight.clone());
        }
    }
    
    (cached, uncached)
}

/// Process scenes that have cached raw segments (local or R2).
async fn process_scenes_with_cache(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    work_dir: &PathBuf,
    clip_tasks: &[ClipTask],
    cached_scenes: &[Highlight],
    highlights: &vclip_models::VideoHighlights,
    total_clips: usize,
) -> WorkerResult<u32> {
    if cached_scenes.is_empty() {
        return Ok(0);
    }
    
    // Filter clip tasks to only cached scenes
    let cached_scene_ids: std::collections::HashSet<_> = cached_scenes.iter().map(|h| h.id).collect();
    let cached_tasks: Vec<ClipTask> = clip_tasks
        .iter()
        .filter(|t| cached_scene_ids.contains(&t.scene_id))
        .cloned()
        .collect();
    
    if cached_tasks.is_empty() {
        return Ok(0);
    }
    
    info!(
        video_id = %job.video_id,
        scene_count = cached_scenes.len(),
        task_count = cached_tasks.len(),
        "Processing cached scenes"
    );
    
    // Use a placeholder video file - raw segments will be downloaded from R2
    let placeholder_video = work_dir.join("source.mp4");
    
    process_selected_scenes(
        ctx,
        job,
        clips_dir,
        &placeholder_video,
        &cached_tasks,
        highlights,
        total_clips,
    )
    .await
}

/// Process uncached scenes with optimized acquisition strategy:
/// 1. Try yt-dlp segment download for each scene (parallel)
/// 2. For scenes where yt-dlp fails, download full source and extract
async fn process_uncached_scenes_optimized(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    work_dir: &PathBuf,
    clip_tasks: &[ClipTask],
    uncached_scenes: &[Highlight],
    highlights: &vclip_models::VideoHighlights,
    video_url: Option<&str>,
    total_clips: usize,
) -> WorkerResult<u32> {
    use vclip_media::intelligent::parse_timestamp;
    use crate::raw_segment_cache::raw_segment_r2_key;
    
    if uncached_scenes.is_empty() {
        return Ok(0);
    }
    
    // Filter clip tasks to only uncached scenes
    let uncached_scene_ids: std::collections::HashSet<_> = uncached_scenes.iter().map(|h| h.id).collect();
    let uncached_tasks: Vec<ClipTask> = clip_tasks
        .iter()
        .filter(|t| uncached_scene_ids.contains(&t.scene_id))
        .cloned()
        .collect();
    
    if uncached_tasks.is_empty() {
        return Ok(0);
    }
    
    info!(
        video_id = %job.video_id,
        scene_count = uncached_scenes.len(),
        task_count = uncached_tasks.len(),
        "Processing uncached scenes with optimized acquisition"
    );
    
    // Track which scenes we successfully downloaded via yt-dlp
    let mut ytdlp_success_scenes: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut ytdlp_failed_scenes: Vec<&Highlight> = Vec::new();
    
    // Try yt-dlp segment download for each uncached scene
    if let Some(url) = video_url {
        if vclip_media::likely_supports_segment_download(url) {
            ctx.progress
                .log(&job.job_id, "Trying direct segment downloads (yt-dlp)...")
                .await
                .ok();
            
            for highlight in uncached_scenes {
                let scene_id = highlight.id;
                let raw_segment = work_dir.join(format!("raw_{}.mp4", scene_id));
                
                // Calculate padded timestamps
                let start_secs = parse_timestamp(&highlight.start).unwrap_or(0.0);
                let end_secs = parse_timestamp(&highlight.end).unwrap_or(30.0);
                let pad_before = highlight.pad_before;
                let pad_after = highlight.pad_after;
                let padded_start = (start_secs - pad_before).max(0.0);
                let padded_end = end_secs + pad_after;
                
                info!(
                    scene_id = scene_id,
                    url = %url,
                    start = padded_start,
                    end = padded_end,
                    "Trying yt-dlp segment download"
                );
                
                match vclip_media::download_segment(
                    url,
                    padded_start,
                    padded_end,
                    &raw_segment,
                    true, // force_keyframes for accurate cuts
                )
                .await
                {
                    Ok(()) => {
                        info!(
                            scene_id = scene_id,
                            "Downloaded segment directly via yt-dlp"
                        );
                        ytdlp_success_scenes.insert(scene_id);
                        
                        // Upload to R2 for future use (non-blocking, fire-and-forget)
                        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
                        if let Err(e) = ctx.raw_cache.upload_raw_segment(&raw_segment, &r2_key).await {
                            warn!(
                                scene_id = scene_id,
                                error = %e,
                                "Failed to upload yt-dlp segment to R2 (non-critical)"
                            );
                        }
                    }
                    Err(e) => {
                        if e.downcast_ref::<vclip_media::SegmentDownloadNotSupported>().is_some() {
                            info!(
                                scene_id = scene_id,
                                "yt-dlp segment download not supported, will use full source"
                            );
                        } else {
                            warn!(
                                scene_id = scene_id,
                                error = %e,
                                "yt-dlp segment download failed, will use full source"
                            );
                        }
                        ytdlp_failed_scenes.push(highlight);
                    }
                }
            }
        } else {
            // URL doesn't support segment download, all scenes need full source
            ytdlp_failed_scenes = uncached_scenes.iter().collect();
        }
    } else {
        // No URL available, all scenes need full source
        ytdlp_failed_scenes = uncached_scenes.iter().collect();
    }
    
    // If any scenes failed yt-dlp, download full source and extract
    if !ytdlp_failed_scenes.is_empty() {
        info!(
            video_id = %job.video_id,
            failed_count = ytdlp_failed_scenes.len(),
            "Downloading full source for scenes that failed yt-dlp"
        );
        
        ctx.progress
            .log(&job.job_id, "Downloading full source video...")
            .await
            .ok();
        
        let video_file = download_source_video(ctx, job, work_dir, highlights).await?;
        
        // Extract raw segments for failed scenes
        for highlight in &ytdlp_failed_scenes {
            let scene_id = highlight.id;
            let raw_segment = work_dir.join(format!("raw_{}.mp4", scene_id));
            
            // Skip if already exists (shouldn't happen, but be safe)
            if raw_segment.exists() {
                continue;
            }
            
            let start_secs = parse_timestamp(&highlight.start).unwrap_or(0.0);
            let end_secs = parse_timestamp(&highlight.end).unwrap_or(30.0);
            let pad_before = highlight.pad_before;
            let pad_after = highlight.pad_after;
            let padded_start = (start_secs - pad_before).max(0.0);
            let padded_end = end_secs + pad_after;
            let padded_start_ts = format_timestamp(padded_start);
            let padded_end_ts = format_timestamp(padded_end);
            
            let (_seg, _created) = ctx
                .raw_cache
                .get_or_create_with_outcome(
                    &job.user_id,
                    job.video_id.as_str(),
                    scene_id,
                    &video_file,
                    &padded_start_ts,
                    &padded_end_ts,
                    work_dir,
                )
                .await?;
        }
    }
    
    // Now process all uncached scenes (they all have raw segments now)
    let placeholder_video = work_dir.join("source.mp4");
    
    process_selected_scenes(
        ctx,
        job,
        clips_dir,
        &placeholder_video,
        &uncached_tasks,
        highlights,
        total_clips,
    )
    .await
}

/// Download source video from R2 (cached) or original URL (fallback).
///
/// Uses `SourceVideoDownloadCoordinator` to prevent duplicate downloads:
/// 1. Check if source already exists in local work directory
/// 2. Use coordinator to check cache or detect in-progress download
/// 3. If another worker is downloading, wait for completion
/// 4. If cache available, download from R2
/// 5. Fall back to original video URL with lock held
async fn download_source_video(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    work_dir: &PathBuf,
    highlights: &vclip_models::VideoHighlights,
) -> WorkerResult<PathBuf> {
    use crate::download_coordinator::{DownloadAction, SourceVideoDownloadCoordinator, WaitResult};
    
    let video_file = work_dir.join("source.mp4");

    // Check if source already exists in local work directory (from previous/concurrent job)
    if video_file.exists() {
        if let Ok(metadata) = tokio::fs::metadata(&video_file).await {
            if metadata.len() > 0 {
                info!(
                    video_id = %job.video_id,
                    path = ?video_file,
                    size_mb = metadata.len() as f64 / 1_048_576.0,
                    "Using existing source video from local work directory"
                );
                return Ok(video_file);
            }
        }
    }

    ctx.progress
        .log(&job.job_id, "Checking source video status...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    // Use coordinator to handle download coordination
    let coordinator = SourceVideoDownloadCoordinator::new(
        ctx.redis.clone(),
        ctx.firestore.clone(),
    );
    
    let action = coordinator
        .acquire_or_wait_for_download(&job.user_id, job.video_id.as_str())
        .await?;

    match action {
        DownloadAction::UseCache { r2_key } => {
            // Download from R2 cache
            ctx.progress
                .log(&job.job_id, "Downloading from cache...")
                .await
                .ok();
            
            match ctx.storage.download_file(&r2_key, &video_file).await {
                Ok(_) => {
                    info!(
                        video_id = %job.video_id,
                        r2_key = r2_key.as_str(),
                        "Downloaded source video from R2 cache"
                    );
                    return Ok(video_file);
                }
                Err(e) => {
                    info!(
                        video_id = %job.video_id,
                        error = %e,
                        "R2 cache download failed, falling back to original URL"
                    );
                }
            }
        }
        
        DownloadAction::WaitForOther => {
            // Another worker is downloading, wait for completion
            ctx.progress
                .log(&job.job_id, "Waiting for background download...")
                .await
                .ok();
            
            let wait_result = coordinator
                .wait_for_download_complete(
                    &job.user_id,
                    job.video_id.as_str(),
                    None, // Use default timeout
                )
                .await?;
            
            match wait_result {
                WaitResult::Ready { r2_key } => {
                    ctx.progress
                        .log(&job.job_id, "Downloading from cache...")
                        .await
                        .ok();
                    
                    match ctx.storage.download_file(&r2_key, &video_file).await {
                        Ok(_) => {
                            info!(
                                video_id = %job.video_id,
                                r2_key = r2_key.as_str(),
                                "Downloaded source video from R2 after waiting for background job"
                            );
                            return Ok(video_file);
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
        }
        
        DownloadAction::PerformDownload { lock_token } => {
            // We acquired the lock, perform the download
            ctx.progress
                .log(&job.job_id, "Downloading source video...")
                .await
                .ok();
            
            // Mark as downloading in Firestore
            coordinator
                .mark_downloading(&job.user_id, job.video_id.as_str())
                .await
                .ok();
            
            if let Some(ref video_url) = highlights.video_url {
                match download_video(video_url, &video_file).await {
                    Ok(_) => {
                        info!(
                            video_id = %job.video_id,
                            url = video_url.as_str(),
                            "Downloaded source video from original URL"
                        );
                        
                        // Upload to R2 for future requests
                        upload_source_to_r2_async(ctx, job, &video_file).await;
                        
                        // Release lock
                        coordinator
                            .release_lock(&job.user_id, job.video_id.as_str(), &lock_token)
                            .await
                            .ok();
                        
                        return Ok(video_file);
                    }
                    Err(e) => {
                        let err_msg = format!("Download failed: {}", e);
                        coordinator
                            .mark_failed(&job.user_id, job.video_id.as_str(), &err_msg)
                            .await
                            .ok();
                        coordinator
                            .release_lock(&job.user_id, job.video_id.as_str(), &lock_token)
                            .await
                            .ok();
                        
                        ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                        return Err(WorkerError::job_failed(&err_msg));
                    }
                }
            } else {
                // Release lock and return error
                coordinator
                    .release_lock(&job.user_id, job.video_id.as_str(), &lock_token)
                    .await
                    .ok();
            }
        }
    }

    // Try legacy R2 location for backwards compatibility
    let legacy_source_key = format!("{}/{}/source.mp4", job.user_id, job.video_id.as_str());
    match ctx.storage.download_file(&legacy_source_key, &video_file).await {
        Ok(_) => {
            info!(
                video_id = %job.video_id,
                "Downloaded source video from legacy R2 location: {}", 
                legacy_source_key
            );
            return Ok(video_file);
        }
        Err(r2_error) => {
            info!(
                video_id = %job.video_id,
                error = %r2_error,
                "Source video not found in legacy R2 location"
            );
        }
    }

    // Final fallback: try original URL directly
    if let Some(ref video_url) = highlights.video_url {
        ctx.progress
            .log(&job.job_id, "Downloading original video from source URL...")
            .await
            .ok();

        match download_video(video_url, &video_file).await {
            Ok(_) => {
                info!(
                    video_id = %job.video_id,
                    url = video_url.as_str(),
                    "Downloaded source video from original URL (final fallback)"
                );
                upload_source_to_r2_async(ctx, job, &video_file).await;
                return Ok(video_file);
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
async fn upload_source_to_r2_async(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    video_file: &std::path::Path,
) {
    use chrono::{Duration as ChronoDuration, Utc};
    
    let r2_key = format!("sources/{}/{}/source.mp4", job.user_id, job.video_id.as_str());
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    
    // Mark as uploading (non-critical)
    video_repo.set_source_video_downloading(&job.video_id).await.ok();
    
    // Upload to R2
    match ctx.storage.upload_file(video_file, &r2_key, "video/mp4").await {
        Ok(_) => {
            info!(
                video_id = %job.video_id,
                r2_key = %r2_key,
                "Uploaded source video to R2 cache"
            );
            
            // Calculate expiration time (24 hours)
            let expires_at = Utc::now() + ChronoDuration::hours(24);
            
            // Mark as ready in Firestore (non-critical)
            if let Err(e) = video_repo.set_source_video_ready(&job.video_id, &r2_key, expires_at).await {
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
            video_repo.set_source_video_failed(&job.video_id, Some(&e.to_string())).await.ok();
        }
    }
}

/// Process all selected scenes with parallel style processing.
///
/// Phase 4: Uses raw segment caching to avoid re-extracting segments
/// when rendering multiple styles for the same scene.
async fn process_selected_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    video_file: &PathBuf,
    clip_tasks: &[ClipTask],
    highlights: &vclip_models::VideoHighlights,
    total_clips: usize,
) -> WorkerResult<u32> {
    use vclip_media::intelligent::parse_timestamp;
    
    // Load existing completed clips for skip-on-resume.
    // When overwrite is requested, skip this check entirely to force re-rendering.
    let existing_completed: std::collections::HashSet<String> = if job.overwrite {
        info!(
            video_id = %job.video_id,
            "Overwrite mode enabled, will re-render existing clips"
        );
        std::collections::HashSet::new()
    } else {
        match vclip_firestore::ClipRepository::new(
            ctx.firestore.clone(),
            &job.user_id,
            job.video_id.clone(),
        )
        .list(Some(ClipStatus::Completed))
        .await
        {
            Ok(clips) => clips.into_iter().map(|c| c.clip_id).collect(),
            Err(e) => {
                tracing::info!("Failed to list completed clips (processing all): {}", e);
                std::collections::HashSet::new()
            }
        }
    };

    // Group clips by scene_id for parallel processing of styles within each scene
    let mut scene_groups: HashMap<u32, Vec<&ClipTask>> = HashMap::new();
    for task in clip_tasks {
        scene_groups.entry(task.scene_id).or_default().push(task);
    }

    // Sort scenes by scene_id for deterministic ordering
    let mut scene_ids: Vec<u32> = scene_groups.keys().copied().collect();
    scene_ids.sort();

    // Get work_dir from clips_dir parent
    let work_dir = clips_dir.parent().unwrap_or(clips_dir);

    // Process clips with parallel style processing within each scene
    let mut completed_clips = 0u32;
    let mut processed_count = 0usize;

    for scene_id in scene_ids {
        let scene_tasks = scene_groups.get(&scene_id).unwrap();
        let first_task = scene_tasks[0];

        ctx.progress
            .log(
                &job.job_id,
                format!(
                    "Processing scene {} ({} styles in parallel)...",
                    scene_id,
                    scene_tasks.len()
                ),
            )
            .await
            .ok();

        // Parse original timestamps for progress display (BEFORE raw segment timestamp modification)
        let original_start_secs = parse_timestamp(&first_task.start).unwrap_or(0.0);
        let original_end_secs = parse_timestamp(&first_task.end).unwrap_or(30.0);
        let original_duration = original_end_secs - original_start_secs + first_task.pad_before + first_task.pad_after;

        // Emit scene_started with ORIGINAL video timestamps so frontend shows correct timing
        if let Err(e) = ctx
            .progress
            .scene_started(
                &job.job_id,
                scene_id,
                &first_task.scene_title,
                scene_tasks.len() as u32,
                original_start_secs,
                original_duration,
            )
            .await
        {
            tracing::warn!(
                scene_id = scene_id,
                error = %e,
                "Failed to emit scene_started event"
            );
        }

        // Phase 4: Get or create cached raw segment for this scene
        // This avoids re-extracting the segment for each style
        let pad_before = first_task.pad_before;
        let pad_after = first_task.pad_after;
        
        let start_secs = parse_timestamp(&first_task.start).unwrap_or(0.0);
        let end_secs = parse_timestamp(&first_task.end).unwrap_or(30.0);
        let padded_start = (start_secs - pad_before).max(0.0);
        let padded_end = end_secs + pad_after;
        
        let padded_start_ts = format_timestamp(padded_start);
        let padded_end_ts = format_timestamp(padded_end);

        let (raw_segment, raw_created) = ctx
            .raw_cache
            .get_or_create_with_outcome(
                &job.user_id,
                job.video_id.as_str(),
                scene_id,
                video_file,
                &padded_start_ts,
                &padded_end_ts,
                work_dir,
            )
            .await?;

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
            scene_id = scene_id,
            raw_segment = ?raw_segment,
            raw_created = raw_created,
            "Using raw segment for scene processing"
        );

        // Create modified tasks that use the raw segment (timestamps relative to segment start)
        let segment_duration = padded_end - padded_start;
        let modified_tasks: Vec<ClipTask> = scene_tasks
            .iter()
            .map(|task| ClipTask {
                scene_id: task.scene_id,
                scene_title: task.scene_title.clone(),
                scene_description: task.scene_description.clone(),
                // When using raw segment, timestamps are relative to segment start
                start: "00:00:00".to_string(),
                end: format_timestamp(segment_duration),
                style: task.style,
                crop_mode: task.crop_mode.clone(),
                target_aspect: task.target_aspect.clone(),
                priority: task.priority,
                // Padding already applied in raw extraction
                pad_before: 0.0,
                pad_after: 0.0,
                streamer_split_params: task.streamer_split_params.clone(),
            })
            .collect();

        let modified_task_refs: Vec<&ClipTask> = modified_tasks.iter().collect();

        // Create a temporary ProcessVideoJob for the scene processor
        let temp_job = vclip_queue::ProcessVideoJob {
            job_id: job.job_id.clone(),
            user_id: job.user_id.clone(),
            video_id: job.video_id.clone(),
            video_url: highlights.video_url.clone().unwrap_or_default(),
            styles: job.styles.clone(),
            crop_mode: job.crop_mode.clone(),
            target_aspect: job.target_aspect.clone(),
            custom_prompt: None,
        };

        // Process scene using the raw segment instead of full source
        // Phase 4 fix: set raw_r2_key atomically during clip creation
        let raw_key = crate::raw_segment_cache::raw_segment_r2_key(
            &job.user_id,
            job.video_id.as_str(),
            scene_id,
        );

        let scene_results = clip_pipeline::scene::process_scene_with_raw_key(
            ctx,
            &temp_job,
            clips_dir,
            &raw_segment, // Use raw segment instead of full video
            &modified_task_refs,
            &existing_completed,
            total_clips,
            Some(raw_key),
            true, // Skip scene_started - we already emitted with original timestamps above
        )
        .await?;

        processed_count += scene_results.processed;
        completed_clips += scene_results.completed;

        // Update progress after each scene (25% to 95%)
        let progress = 25 + (processed_count * 70 / total_clips) as u32;
        ctx.progress
            .progress(&job.job_id, progress as u8)
            .await
            .ok();
    }

    Ok(completed_clips)
}

/// Format seconds as HH:MM:SS.mmm timestamp for FFmpeg.
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}

/// Update video clip count in Firestore.
async fn update_video_clip_count(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    completed_clips: u32,
) -> WorkerResult<()> {
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

    match video_repo.get(&job.video_id).await {
        Ok(Some(video)) => {
            let new_total = video.clips_count + completed_clips;
            video_repo
                .complete(&job.video_id, new_total)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
            info!(
                "Updated video {} clip count: {} -> {}",
                job.video_id, video.clips_count, new_total
            );
        }
        Ok(None) => {
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
        Err(e) => {
            tracing::warn!("Failed to get video metadata: {}", e);
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
    }

    Ok(())
}
