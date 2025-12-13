//! Scene processing with centralized neural analysis caching.
//!
//! This module processes all styles for a scene, ensuring neural analysis
//! runs ONCE and is cached before parallel style processing begins.
//!
//! # Split Style Optimization
//!
//! When both `intelligent_speaker` and `intelligent_split_speaker` are requested,
//! we pre-compute whether split is appropriate. If split isn't appropriate (e.g.,
//! alternating speakers, single face), the split style would fall back to full-frame
//! rendering - producing identical output to `intelligent_speaker`. In this case,
//! we skip the split style entirely to avoid redundant processing.

use std::path::Path;

use futures::future::join_all;
use tracing::{debug, info, warn};
use vclip_models::{ClipTask, Style};
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

    // Track whether split is appropriate (for optimization)
    let mut split_is_appropriate: Option<bool> = None;

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

                    // Pre-compute split appropriateness for optimization
                    // This allows us to skip redundant split processing when it would
                    // produce identical output to full-frame processing
                    if has_split_and_fullframe_pair(&styles) {
                        split_is_appropriate =
                            Some(SceneAnalysisService::is_split_appropriate(&analysis));
                        info!(
                            scene_id = scene_id,
                            split_appropriate = split_is_appropriate.unwrap(),
                            "Pre-computed split appropriateness for optimization"
                        );
                    }
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

    // Filter out redundant split styles when split isn't appropriate
    // This optimization prevents duplicate full-frame rendering
    let tasks_to_process: Vec<_> = scene_tasks
        .iter()
        .filter(|task| {
            // If we determined split isn't appropriate and this is a split style
            // that has a corresponding full-frame style also being processed,
            // skip the split style to avoid redundant work
            if let Some(false) = split_is_appropriate {
                if is_intelligent_split_style(task.style) {
                    let has_fullframe_counterpart = scene_tasks
                        .iter()
                        .any(|t| is_fullframe_counterpart(task.style, t.style));
                    if has_fullframe_counterpart {
                        warn!(
                            scene_id = scene_id,
                            style = %task.style,
                            "Skipping split style (would produce identical output to full-frame)"
                        );
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect();

    let futures: Vec<_> = tasks_to_process
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

// ============================================================================
// Split Style Optimization Helpers
// ============================================================================

/// Check if a style is an intelligent split style.
fn is_intelligent_split_style(style: Style) -> bool {
    matches!(
        style,
        Style::IntelligentSplit
            | Style::IntelligentSplitSpeaker
            | Style::IntelligentSplitMotion
            | Style::IntelligentSplitActivity
    )
}

/// Check if the styles include both a split style and its full-frame counterpart.
fn has_split_and_fullframe_pair(styles: &[Style]) -> bool {
    let has_split = styles.iter().any(|s| is_intelligent_split_style(*s));
    let has_fullframe = styles.iter().any(|s| {
        matches!(
            s,
            Style::Intelligent | Style::IntelligentSpeaker | Style::IntelligentMotion
        )
    });
    has_split && has_fullframe
}

/// Check if a full-frame style is the counterpart to a split style.
fn is_fullframe_counterpart(split_style: Style, other_style: Style) -> bool {
    match split_style {
        Style::IntelligentSplit => other_style == Style::Intelligent,
        Style::IntelligentSplitSpeaker => other_style == Style::IntelligentSpeaker,
        Style::IntelligentSplitMotion => other_style == Style::IntelligentMotion,
        Style::IntelligentSplitActivity => other_style == Style::IntelligentMotion, // Activity uses motion tier
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_intelligent_split_style() {
        assert!(is_intelligent_split_style(Style::IntelligentSplit));
        assert!(is_intelligent_split_style(Style::IntelligentSplitSpeaker));
        assert!(is_intelligent_split_style(Style::IntelligentSplitMotion));
        assert!(is_intelligent_split_style(Style::IntelligentSplitActivity));

        assert!(!is_intelligent_split_style(Style::Intelligent));
        assert!(!is_intelligent_split_style(Style::IntelligentSpeaker));
        assert!(!is_intelligent_split_style(Style::Split));
        assert!(!is_intelligent_split_style(Style::Original));
    }

    #[test]
    fn test_has_split_and_fullframe_pair() {
        // Has both split and fullframe
        assert!(has_split_and_fullframe_pair(&[
            Style::IntelligentSpeaker,
            Style::IntelligentSplitSpeaker
        ]));
        assert!(has_split_and_fullframe_pair(&[
            Style::Intelligent,
            Style::IntelligentSplit
        ]));

        // Only split styles
        assert!(!has_split_and_fullframe_pair(&[
            Style::IntelligentSplit,
            Style::IntelligentSplitSpeaker
        ]));

        // Only fullframe styles
        assert!(!has_split_and_fullframe_pair(&[
            Style::Intelligent,
            Style::IntelligentSpeaker
        ]));

        // Non-intelligent styles
        assert!(!has_split_and_fullframe_pair(&[
            Style::Split,
            Style::Original
        ]));

        // Empty
        assert!(!has_split_and_fullframe_pair(&[]));
    }

    #[test]
    fn test_is_fullframe_counterpart() {
        // Correct pairs
        assert!(is_fullframe_counterpart(
            Style::IntelligentSplit,
            Style::Intelligent
        ));
        assert!(is_fullframe_counterpart(
            Style::IntelligentSplitSpeaker,
            Style::IntelligentSpeaker
        ));
        assert!(is_fullframe_counterpart(
            Style::IntelligentSplitMotion,
            Style::IntelligentMotion
        ));
        assert!(is_fullframe_counterpart(
            Style::IntelligentSplitActivity,
            Style::IntelligentMotion
        ));

        // Wrong pairs
        assert!(!is_fullframe_counterpart(
            Style::IntelligentSplit,
            Style::IntelligentSpeaker
        ));
        assert!(!is_fullframe_counterpart(
            Style::IntelligentSplitSpeaker,
            Style::Intelligent
        ));

        // Non-split styles
        assert!(!is_fullframe_counterpart(Style::Intelligent, Style::Split));
        assert!(!is_fullframe_counterpart(Style::Split, Style::Intelligent));
    }
}
