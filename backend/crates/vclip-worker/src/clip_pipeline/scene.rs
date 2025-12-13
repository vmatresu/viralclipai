//! Scene processing with centralized neural analysis caching.
//!
//! This module processes all styles for a scene, ensuring neural analysis
//! runs ONCE and is cached before parallel style processing begins.

use std::path::Path;

use futures::future::join_all;
use tracing::{debug, info};
use vclip_models::ClipTask;
use vclip_queue::ProcessVideoJob;

use crate::clip_pipeline::clip::{
    clip_id, compute_padded_timing, process_single_clip, process_single_clip_with_raw_key,
};
use crate::error::WorkerResult;
use crate::processor::EnhancedProcessingContext;
use crate::scene_analysis::SceneAnalysisService;

pub struct SceneProcessingResults {
    pub processed: usize,
    pub completed: u32,
    pub skipped: usize,
}

/// Process a single scene with parallel style processing.
pub async fn process_scene(
    ctx: &EnhancedProcessingContext,
    job: &ProcessVideoJob,
    clips_dir: &Path,
    video_file: &Path,
    scene_tasks: &[&ClipTask],
    existing_completed: &std::collections::HashSet<String>,
    total_clips: usize,
) -> WorkerResult<SceneProcessingResults> {
    process_scene_with_raw_key(
        ctx,
        job,
        clips_dir,
        video_file,
        scene_tasks,
        existing_completed,
        total_clips,
        None,
    )
    .await
}

/// Process a single scene with optional raw segment R2 key.
///
/// # Architecture
///
/// This function implements a two-phase approach:
///
/// 1. **Pre-cache Phase**: Before processing any styles, we ensure neural analysis
///    is cached at the highest tier required by any style. This runs detection
///    ONCE and stores results for all styles to consume.
///
/// 2. **Parallel Processing Phase**: All styles process in parallel, each fetching
///    the cached analysis. No duplicate detection occurs.
///
/// This architecture ensures:
/// - Detection runs exactly ONCE per scene (not per style)
/// - All styles benefit from cached analysis
/// - Parallel processing is safe (no race conditions)
pub async fn process_scene_with_raw_key(
    ctx: &EnhancedProcessingContext,
    job: &ProcessVideoJob,
    clips_dir: &Path,
    video_file: &Path,
    scene_tasks: &[&ClipTask],
    existing_completed: &std::collections::HashSet<String>,
    total_clips: usize,
    raw_r2_key: Option<String>,
) -> WorkerResult<SceneProcessingResults> {
    let first_task = scene_tasks[0];
    let scene_id = first_task.scene_id;

    // Parse timing for scene started event
    let (start_sec, end_sec, duration_sec) = compute_padded_timing(first_task);

    // Emit scene started event
    if let Err(e) = ctx
        .progress
        .scene_started(
            &job.job_id,
            scene_id,
            &first_task.scene_title,
            scene_tasks.len() as u32,
            start_sec,
            duration_sec,
        )
        .await
    {
        tracing::warn!(
            scene_id = scene_id,
            error = %e,
            "Failed to emit scene_started event"
        );
    }

    info!(
        scene_id = scene_id,
        scene_title = %first_task.scene_title,
        styles_count = scene_tasks.len(),
        start_sec = start_sec,
        duration_sec = duration_sec,
        "Starting scene processing"
    );

    ctx.progress
        .log(
            &job.job_id,
            format!(
                "Processing scene {} '{}' ({} styles)...",
                scene_id,
                first_task.scene_title,
                scene_tasks.len()
            ),
        )
        .await
        .ok();

    // Partition tasks into pending and skipped
    let (pending_tasks, skipped_tasks): (Vec<&ClipTask>, Vec<&ClipTask>) = scene_tasks
        .iter()
        .cloned()
        .partition(|task| !existing_completed.contains(&clip_id(&job.video_id, task)));

    if !skipped_tasks.is_empty() {
        info!(
            scene_id = scene_id,
            skipped = skipped_tasks.len(),
            total = scene_tasks.len(),
            "Skipping already-completed clips for scene"
        );
    }

    // If all styles are already done, return early
    if pending_tasks.is_empty() {
        if let Err(e) = ctx
            .progress
            .scene_completed(&job.job_id, scene_id, scene_tasks.len() as u32, 0)
            .await
        {
            tracing::warn!(
                scene_id = scene_id,
                error = %e,
                "Failed to emit scene_completed event for skipped scene"
            );
        }

        return Ok(SceneProcessingResults {
            processed: scene_tasks.len(),
            completed: scene_tasks.len() as u32,
            skipped: scene_tasks.len(),
        });
    }

    // =========================================================================
    // PHASE 1: Pre-cache neural analysis BEFORE parallel processing
    // =========================================================================
    // This ensures detection runs ONCE, not once per style
    
    let styles: Vec<_> = pending_tasks.iter().map(|t| t.style).collect();
    
    if SceneAnalysisService::any_style_uses_cache(&styles) {
        let highest_tier = SceneAnalysisService::highest_required_tier(&styles);
        
        if highest_tier.requires_yunet() {
            info!(
                scene_id = scene_id,
                tier = %highest_tier,
                styles = ?styles,
                "Pre-caching neural analysis for scene (runs detection ONCE)"
            );

            let analysis_service = SceneAnalysisService::new(ctx);
            
            // This will either:
            // - Return immediately if cache exists
            // - Run detection ONCE and cache results
            // - Use Redis lock to prevent duplicate computation
            match analysis_service
                .ensure_analysis_cached(
                    &job.user_id,
                    job.video_id.as_str(),
                    scene_id,
                    video_file,
                    start_sec,
                    end_sec,
                    highest_tier,
                )
                .await
            {
                Ok(analysis) => {
                    info!(
                        scene_id = scene_id,
                        frames = analysis.frames.len(),
                        tier = %highest_tier,
                        "Neural analysis cached, proceeding with parallel style processing"
                    );
                }
                Err(e) => {
                    // Non-fatal: styles will run detection inline if cache fails
                    tracing::warn!(
                        scene_id = scene_id,
                        error = %e,
                        "Failed to pre-cache neural analysis, styles will run detection inline"
                    );
                }
            }
        } else {
            debug!(
                scene_id = scene_id,
                tier = %highest_tier,
                "Skipping pre-cache: tier does not require YuNet"
            );
        }
    }

    // =========================================================================
    // PHASE 2: Process all styles in parallel (using cached analysis)
    // =========================================================================
    
    let futures: Vec<_> = scene_tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let ctx = ctx;
            let job_id = &job.job_id;
            let video_id = &job.video_id;
            let user_id = &job.user_id;
            let video_file = video_file;
            let clips_dir = clips_dir;
            let raw_r2_key = raw_r2_key.clone();

            async move {
                if existing_completed.contains(&clip_id(video_id, task)) {
                    return Ok(());
                }
                match &raw_r2_key {
                    Some(key) => {
                        process_single_clip_with_raw_key(
                            ctx,
                            job_id,
                            video_id,
                            user_id,
                            video_file,
                            clips_dir,
                            task,
                            idx,
                            total_clips,
                            Some(key.clone()),
                        )
                        .await
                    }
                    None => {
                        process_single_clip(
                            ctx,
                            job_id,
                            video_id,
                            user_id,
                            video_file,
                            clips_dir,
                            task,
                            idx,
                            total_clips,
                        )
                        .await
                    }
                }
            }
        })
        .collect();

    let results = join_all(futures).await;

    // Aggregate results
    let mut processed: usize = 0;
    let mut completed: usize = 0;
    let mut errors = Vec::new();

    for (idx, result) in results.into_iter().enumerate() {
        match result {
            Ok(_) => {
                processed += 1;
                completed += 1;
            }
            Err(e) => {
                processed += 1;
                errors.push((idx, e));
            }
        }
    }

    // Emit scene summary event
    if let Err(e) = ctx
        .progress
        .scene_completed(&job.job_id, scene_id, completed as u32, errors.len() as u32)
        .await
    {
        tracing::warn!(
            scene_id = scene_id,
            error = %e,
            "Failed to emit scene_completed event"
        );
    }

    // Log errors
    if !errors.is_empty() {
        for (idx, err) in errors {
            tracing::error!(
                scene_id = scene_id,
                clip_index = idx + 1,
                total = scene_tasks.len(),
                error = %err,
                "Clip processing failed"
            );
        }
    }

    Ok(SceneProcessingResults {
        processed: processed + skipped_tasks.len(),
        completed: completed as u32 + skipped_tasks.len() as u32,
        skipped: skipped_tasks.len(),
    })
}
