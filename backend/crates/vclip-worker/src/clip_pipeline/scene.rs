use std::path::Path;

use futures::future::join_all;
use vclip_models::ClipTask;
use vclip_queue::ProcessVideoJob;

use crate::clip_pipeline::clip::{compute_padded_timing, process_single_clip};
use crate::error::WorkerResult;
use crate::processor::EnhancedProcessingContext;

pub struct SceneProcessingResults {
    pub processed: usize,
    pub completed: u32,
}

/// Process a single scene with parallel style processing.
pub async fn process_scene(
    ctx: &EnhancedProcessingContext,
    job: &ProcessVideoJob,
    clips_dir: &Path,
    video_file: &Path,
    scene_tasks: &[&ClipTask],
    total_clips: usize,
) -> WorkerResult<SceneProcessingResults> {
    let first_task = scene_tasks[0];
    let scene_id = first_task.scene_id;

    // Parse timing for scene started event (defensive: default to safe values)
    let (start_sec, _end_sec, duration_sec) = compute_padded_timing(first_task);

    // Emit scene started event with structured data
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

    // Structured logging for observability
    tracing::info!(
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
                "Processing scene {} '{}' ({} styles in parallel)...",
                scene_id,
                first_task.scene_title,
                scene_tasks.len()
            ),
        )
        .await
        .ok();

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

            async move {
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
        })
        .collect();

    let results = join_all(futures).await;

    // Aggregate results with detailed error tracking
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
        .scene_completed(&job.job_id, scene_id, completed as u32, scene_tasks.len() as u32)
        .await
    {
        tracing::warn!(
            scene_id = scene_id,
            error = %e,
            "Failed to emit scene_completed event"
        );
    }

    // Log errors with context
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
        processed,
        completed: completed as u32,
    })
}

