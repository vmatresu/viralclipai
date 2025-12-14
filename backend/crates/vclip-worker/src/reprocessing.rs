//! Scene reprocessing logic.
//!
//! This module handles the reprocessing of specific scenes from a previously
//! processed video, allowing users to apply different styles without re-running
//! the AI analysis.

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::info;

use vclip_media::download_video;
use vclip_models::{ClipStatus, ClipTask};
use vclip_queue::ReprocessScenesJob;

use crate::clip_pipeline;
use crate::error::{WorkerError, WorkerResult};
use crate::processor::{EnhancedProcessingContext, JobLogger};

/// Process a reprocess scenes job.
///
/// This is different from `process_video_job` as it:
/// 1. Loads existing highlights from Firestore (doesn't re-run AI analysis)
/// 2. Filters to only the requested scene IDs
/// 3. Downloads video from R2 or original URL (doesn't re-download from YouTube)
/// 4. Only processes the selected scenes with the requested styles
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

    // CACHE-FIRST: Check if all raw segments exist in R2 before downloading full source
    let all_scenes_cached = check_all_raw_segments_cached(ctx, job, &job.scene_ids).await;
    
    // Only download full source if any scene is missing from cache
    let video_file = if all_scenes_cached {
        info!(
            video_id = %job.video_id,
            scenes = ?job.scene_ids,
            "All raw segments cached in R2, skipping full source download"
        );
        // Return a placeholder path - process_selected_scenes will download raw segments directly
        work_dir.join("source.mp4")
    } else {
        info!(
            video_id = %job.video_id,
            "Some raw segments not cached, downloading source video..."
        );
        download_source_video(ctx, job, &work_dir, &video_highlights).await?
    };

    ctx.progress.progress(&job.job_id, 25).await.ok();

    // Generate clip tasks from selected highlights only (using Firestore model)
    let clip_tasks = clip_pipeline::tasks::generate_clip_tasks_from_firestore_highlights(
        &selected_highlights,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
    );

    ctx.progress
        .log(&job.job_id, format!("Generating {} clips...", total_clips))
        .await
        .ok();

    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Process scenes
    let completed_clips = process_selected_scenes(
        ctx,
        job,
        &clips_dir,
        &video_file,
        &clip_tasks,
        &video_highlights,
        total_clips,
    )
    .await?;

    // Update video metadata - add new clips to existing count
    update_video_clip_count(ctx, job, completed_clips).await?;

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
        completed_clips
    ));

    Ok(())
}

/// Download source video from R2 (cached) or original URL (fallback).
///
/// Priority:
/// 1. Check if source already exists in local work directory (from previous job)
/// 2. Check Firestore for source_video_status == Ready and valid R2 key not expired
/// 3. Try R2 download from cached source location
/// 4. Fall back to original video URL and upload to R2 for future use
async fn download_source_video(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    work_dir: &PathBuf,
    highlights: &vclip_models::VideoHighlights,
) -> WorkerResult<PathBuf> {
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
        .log(&job.job_id, "Downloading source video...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    // First, check Firestore for cached source video status
    // IMPORTANT: Always try R2 if we have a key, even if Firestore says "expired"
    // because R2 objects don't actually expire unless lifecycle rules are configured.
    // The expires_at in Firestore is just metadata tracking, not actual object TTL.
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    let (use_cached_source, is_expired) = match video_repo.get(&job.video_id).await {
        Ok(Some(video_meta)) => {
            // Try R2 download even if "expired" in Firestore - object may still exist
            if let (Some(status), Some(ref r2_key)) = (
                video_meta.source_video_status,
                &video_meta.source_video_r2_key,
            ) {
                if status == vclip_models::SourceVideoStatus::Ready {
                    let expired = video_meta.source_video_expires_at
                        .map(|exp| exp <= chrono::Utc::now())
                        .unwrap_or(false);
                    info!(
                        "Using cached source video from R2: {} (expired_in_metadata: {})",
                        r2_key, expired
                    );
                    (Some(r2_key.clone()), expired)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            }
        }
        _ => (None, false),
    };

    // Try cached R2 source if available (even if metadata says expired)
    if let Some(r2_key) = use_cached_source {
        match ctx.storage.download_file(&r2_key, &video_file).await {
            Ok(_) => {
                info!("Downloaded source video from R2 cache: {}", r2_key);
                return Ok(video_file);
            }
            Err(e) => {
                info!("Failed to download from R2 key {}: {}, trying fallbacks", r2_key, e);
                // Only mark as expired if R2 download actually fails AND metadata said expired
                if is_expired {
                    video_repo.set_source_video_expired(&job.video_id).await.ok();
                }
            }
        }
    }

    // Try legacy R2 location for backwards compatibility
    let legacy_source_key = format!("{}/{}/source.mp4", job.user_id, job.video_id.as_str());
    match ctx.storage.download_file(&legacy_source_key, &video_file).await {
        Ok(_) => {
            info!("Downloaded source video from legacy R2 location: {}", legacy_source_key);
            return Ok(video_file);
        }
        Err(r2_error) => {
            info!(
                "Source video not found in R2 ({}), trying original URL from highlights",
                r2_error
            );
        }
    }

    // Fall back to original video URL from highlights data
    if let Some(ref video_url) = highlights.video_url {
        ctx.progress
            .log(&job.job_id, "Downloading original video from source URL...")
            .await
            .ok();

        match download_video(video_url, &video_file).await {
            Ok(_) => {
                info!("Downloaded source video from original URL: {} (fallback path)", video_url);

                // Upload to R2 immediately for future requests (avoids duplicate download)
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
    let existing_completed: std::collections::HashSet<String> =
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

/// Check if all raw segments for the given scene IDs are cached locally or in R2.
///
/// Returns true only if ALL scenes have cached raw segments (either local or R2).
/// Checks LOCAL FILES FIRST before checking R2 to avoid unnecessary network calls.
async fn check_all_raw_segments_cached(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    scene_ids: &[u32],
) -> bool {
    use crate::raw_segment_cache::raw_segment_r2_key;

    let work_dir = std::path::PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());

    for scene_id in scene_ids {
        // Check local file first (fast path)
        let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));
        if local_path.exists() {
            tracing::debug!(
                scene_id = scene_id,
                path = ?local_path,
                "Raw segment exists locally"
            );
            continue;
        }

        // Check R2 if not local
        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), *scene_id);
        if !ctx.raw_cache.check_raw_exists(&r2_key).await {
            tracing::info!(
                scene_id = scene_id,
                "Raw segment not cached (not local, not in R2)"
            );
            return false;
        }
    }
    true
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
