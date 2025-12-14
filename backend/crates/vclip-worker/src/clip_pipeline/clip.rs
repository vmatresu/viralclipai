use std::path::Path;

use tracing::{debug, info};
use vclip_firestore::{ClipRepository, FromFirestoreValue, StorageAccountingRepository};
use vclip_media::core::{ProcessingContext as MediaProcessingContext, ProcessingRequest};
use vclip_media::intelligent::parse_timestamp;
use vclip_models::{
    ClipMetadata, ClipProcessingStep, ClipTask, EncodingConfig, JobId, PlanTier, Style, VideoId,
};

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;

const ESTIMATED_CLIP_SIZE_BYTES: u64 = 50 * 1024 * 1024;

/// Deterministic clip id for a scene/style of a video.
pub fn clip_id(video_id: &VideoId, task: &ClipTask) -> String {
    format!("{}_{}_{}", video_id, task.scene_id, task.style)
}

/// Select encoding configuration per style.
fn encoding_for_style(style: Style) -> EncodingConfig {
    match style {
        Style::Intelligent
        | Style::IntelligentSpeaker
        | Style::IntelligentMotion
        | Style::IntelligentSplit
        | Style::IntelligentSplitSpeaker
        | Style::IntelligentSplitMotion
        | Style::IntelligentSplitActivity
        | Style::IntelligentCinematic => EncodingConfig::for_intelligent_crop().with_crf(24),
        // Static Split/Focus styles use higher CRF to shrink output size.
        Style::Split => EncodingConfig::for_split_view().with_crf(24),
        Style::SplitFast => EncodingConfig::for_split_view().with_crf(24),
        Style::LeftFocus => EncodingConfig::for_split_view().with_crf(24),
        Style::RightFocus => EncodingConfig::for_split_view().with_crf(24),
        Style::CenterFocus => EncodingConfig::for_split_view().with_crf(24),
        Style::Original => EncodingConfig::default().with_crf(24),
    }
}

pub(super) fn compute_padded_timing(task: &ClipTask) -> (f64, f64, f64) {
    let raw_start = parse_timestamp(&task.start).unwrap_or(0.0);
    let raw_end = parse_timestamp(&task.end).unwrap_or(30.0);
    let start = (raw_start - task.pad_before).max(0.0);
    let end = raw_end + task.pad_after;
    (start, end, end - start)
}

/// Process a single clip task with full error handling and progress reporting.
///
/// # Arguments
/// * `raw_r2_key` - Optional pre-known raw segment R2 key for atomic clip creation
pub async fn process_single_clip(
    ctx: &EnhancedProcessingContext,
    job_id: &JobId,
    video_id: &VideoId,
    user_id: &str,
    video_file: &Path,
    clips_dir: &Path,
    task: &ClipTask,
    clip_index: usize,
    total_clips: usize,
) -> WorkerResult<()> {
    process_single_clip_with_raw_key(
        ctx,
        job_id,
        video_id,
        user_id,
        video_file,
        clips_dir,
        task,
        clip_index,
        total_clips,
        None, // No pre-known raw key
    )
    .await
}

/// Process a single clip task with optional pre-known raw_r2_key.
///
/// This variant allows setting raw_r2_key atomically during clip creation,
/// avoiding the consistency gap where clips exist without raw linkage.
pub async fn process_single_clip_with_raw_key(
    ctx: &EnhancedProcessingContext,
    job_id: &JobId,
    video_id: &VideoId,
    user_id: &str,
    video_file: &Path,
    clips_dir: &Path,
    task: &ClipTask,
    clip_index: usize,
    total_clips: usize,
    raw_r2_key: Option<String>,
) -> WorkerResult<()> {
    // Phase 5: Enforce quota BEFORE processing (fail fast)
    enforce_quota(ctx, user_id).await?;

    let scene_id = task.scene_id;
    let style_name = task.style.to_string();
    let filename = task.output_filename();
    let output_path = clips_dir.join(&filename);

    // Structured logging for clip start
    tracing::info!(
        scene_id = scene_id,
        style = %style_name,
        clip_index = clip_index + 1,
        total_clips = total_clips,
        filename = %filename,
        "Starting clip processing"
    );

    // Acquire semaphore for resource control (prevents resource exhaustion)
    let _permit = ctx.ffmpeg_semaphore.acquire().await.map_err(|_| {
        let err = WorkerError::job_failed("Failed to acquire FFmpeg permit");
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            "Semaphore acquisition failed"
        );
        err
    })?;

    let (start_sec, end_sec, duration_sec) = compute_padded_timing(task);

    // Helper macro for emitting progress (DRY principle)
    macro_rules! emit_progress {
        ($step:expr, $details:expr) => {{
            let step = $step;
            let details: Option<String> = $details;
            if let Err(e) = ctx
                .progress
                .clip_progress(job_id, scene_id, &style_name, step, details.clone())
                .await
            {
                tracing::warn!(
                    scene_id = scene_id,
                    style = %style_name,
                    step = ?step,
                    error = %e,
                    "Failed to emit progress event"
                );
            }
        }};
    }

    // Stage 1: Extracting segment
    emit_progress!(
        ClipProcessingStep::ExtractingSegment,
        Some(format!(
            "{:.1}s - {:.1}s ({:.1}s)",
            start_sec, end_sec, duration_sec
        ))
    );

    // Create processing request with error context
    let mut request = ProcessingRequest::new(
        task.clone(),
        video_file,
        &output_path,
        encoding_for_style(task.style),
        job_id.to_string(),
        user_id.to_string(),
    )
    .map_err(|e| {
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            error = %e,
            "Failed to create processing request"
        );
        e
    })?;

    // Phase 3: Fetch cached analysis for intelligent styles (face detection OR motion heuristics)
    // This allows skipping expensive per-frame analysis when cache is available
    if task.style.can_use_cached_analysis() {
        let required_tier = task.style.detection_tier();
        match ctx
            .neural_cache
            .get_cached_for_tier(user_id, video_id.as_str(), scene_id, required_tier)
            .await
        {
            Ok(Some(analysis)) => {
                info!(
                    scene_id = scene_id,
                    style = %style_name,
                    frames = analysis.frames.len(),
                    tier = %required_tier,
                    "Using cached analysis (SKIPPING expensive detection)"
                );
                request = request.with_cached_neural_analysis(analysis);
            }
            Ok(None) => {
                debug!(
                    scene_id = scene_id,
                    style = %style_name,
                    tier = %required_tier,
                    "No cached analysis available, will run detection"
                );
            }
            Err(e) => {
                debug!(
                    scene_id = scene_id,
                    style = %style_name,
                    error = %e,
                    "Failed to check cache (will run detection)"
                );
            }
        }
    }

    // Create processing context (dependency injection pattern)
    let proc_ctx = MediaProcessingContext::new(
        request.request_id.clone(),
        request.user_id.clone(),
        clips_dir,
        ctx.ffmpeg_semaphore.clone(),
        ctx.metrics.clone(),
        ctx.security.clone(),
    );

    // Stage 2: Rendering
    emit_progress!(
        ClipProcessingStep::Rendering,
        Some(format!("Style: {} (cached: {})", style_name, request.has_cached_analysis()))
    );

    // Get style processor and process (strategy pattern)
    let processor = ctx
        .style_registry
        .get_processor(task.style)
        .await
        .map_err(|e| {
            tracing::error!(
                scene_id = scene_id,
                style = %style_name,
                error = %e,
                "Failed to get style processor"
            );
            e
        })?;

    let result = processor.process(request, proc_ctx).await.map_err(|e| {
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            error = %e,
            "Style processor failed"
        );
        // Emit failure event
        let _ = ctx.progress.clip_progress(
            job_id,
            scene_id,
            &style_name,
            ClipProcessingStep::Failed,
            Some(format!("Rendering failed: {}", e)),
        );
        e
    })?;

    // Stage 3: Render complete
    emit_progress!(ClipProcessingStep::RenderComplete, None);

    // Stage 4: Uploading
    emit_progress!(ClipProcessingStep::Uploading, Some(filename.clone()));

    // Upload video to storage with error context
    let r2_key = ctx
        .storage
        .upload_clip(&result.output_path, user_id, video_id.as_str(), &filename)
        .await
        .map_err(|e| {
            tracing::error!(
                scene_id = scene_id,
                style = %style_name,
                filename = %filename,
                error = %e,
                "Failed to upload clip to storage"
            );
            // Emit failure event
            let _ = ctx.progress.clip_progress(
                job_id,
                scene_id,
                &style_name,
                ClipProcessingStep::Failed,
                Some(format!("Upload failed: {}", e)),
            );
            WorkerError::Storage(e)
        })?;

    // Upload thumbnail if available (truly non-critical - continue on failure)
    let thumb_key = if let Some(thumb_path) = &result.thumbnail_path {
        if let Ok(file) = tokio::fs::File::open(thumb_path).await {
            if let Err(e) = file.sync_all().await {
                tracing::warn!(
                    scene_id = scene_id,
                    style = %style_name,
                    path = ?thumb_path,
                    error = %e,
                    "Failed to fsync thumbnail before upload"
                );
            }
        }
        let thumb_filename = filename.replace(".mp4", ".jpg");
        match ctx
            .storage
            .upload_clip(thumb_path, user_id, video_id.as_str(), &thumb_filename)
            .await
        {
            Ok(key) => Some(key),
            Err(e) => {
                tracing::warn!(
                    scene_id = scene_id,
                    style = %style_name,
                    error = %e,
                    "Failed to upload thumbnail (non-critical) - continuing without thumbnail"
                );
                None
            }
        }
    } else {
        None
    };

    // Stage 5: Uploaded
    emit_progress!(ClipProcessingStep::UploadComplete, Some(filename.clone()));

    // Emit legacy clip_uploaded message for backward compatibility
    if let Err(e) = ctx
        .progress
        .clip_uploaded(
            job_id,
            video_id.as_str(),
            clip_index as u32 + 1,
            total_clips as u32,
            task.style.credit_cost(),
        )
        .await
    {
        tracing::warn!(
            scene_id = scene_id,
            style = %style_name,
            error = %e,
            "Failed to emit clip_uploaded event"
        );
    }

    // Stage 6: Complete
    emit_progress!(ClipProcessingStep::Complete, None);

    // Create clip metadata with all processing results
    // Phase 4 fix: Set raw_r2_key atomically during creation when available
    let clip_meta = ClipMetadata {
        clip_id: format!("{}_{}_{}", video_id, task.scene_id, task.style),
        video_id: video_id.clone(),
        user_id: user_id.to_string(),
        scene_id: task.scene_id,
        scene_title: task.scene_title.clone(),
        scene_description: task.scene_description.clone(),
        filename: filename.clone(),
        style: task.style.to_string(),
        priority: task.priority,
        start_time: task.start.clone(),
        end_time: task.end.clone(),
        duration_seconds: result.duration_seconds,
        file_size_bytes: result.file_size_bytes,
        file_size_mb: result.file_size_bytes as f64 / (1024.0 * 1024.0),
        has_thumbnail: result.thumbnail_path.is_some(),
        r2_key,
        thumbnail_r2_key: thumb_key,
        raw_r2_key, // Set atomically during creation when provided
        status: vclip_models::ClipStatus::Completed,
        created_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        created_by: user_id.to_string(),
    };

    // Persist clip metadata to Firestore (repository pattern)
    let clip_repo = ClipRepository::new(ctx.firestore.clone(), user_id, video_id.clone());

    clip_repo.create(&clip_meta).await.map_err(|e| {
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            clip_id = %clip_meta.clip_id,
            error = %e,
            "Failed to save clip metadata to Firestore"
        );
        WorkerError::Firestore(e)
    })?;

    // Update video's total size (fire and forget - non-critical)
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), user_id);
    if let Err(e) = video_repo
        .add_clip_size(video_id, result.file_size_bytes)
        .await
    {
        tracing::warn!(
            video_id = %video_id,
            size_bytes = result.file_size_bytes,
            error = %e,
            "Failed to update video total size (non-critical)"
        );
    }

    // Phase 5: Update storage accounting (billable styled clip)
    // StorageAccounting is the CANONICAL source of truth for quota tracking
    let storage_repo = vclip_firestore::StorageAccountingRepository::new(
        ctx.firestore.clone(),
        user_id,
    );
    if let Err(e) = storage_repo.add_styled_clip(result.file_size_bytes).await {
        tracing::warn!(
            user_id = %user_id,
            size_bytes = result.file_size_bytes,
            error = %e,
            "Failed to update storage accounting for styled clip (non-critical)"
        );
    }

    // NOTE: Legacy users.total_storage_bytes and users.total_clips_count updates removed.
    // StorageAccounting ({storage_accounting}/{user_id}) is now the canonical source.
    // See Phase 5 quota tracking documentation.

    // Structured success log
    info!(
        scene_id = scene_id,
        style = %style_name,
        filename = %filename,
        duration_sec = result.duration_seconds,
        file_size_mb = result.file_size_bytes as f64 / (1024.0 * 1024.0),
        "Clip processing completed successfully"
    );

    Ok(())
}

async fn enforce_quota(ctx: &EnhancedProcessingContext, user_id: &str) -> WorkerResult<()> {
    let mut tier = PlanTier::Free;

    if let Ok(Some(doc)) = ctx.firestore.get_document("users", user_id).await {
        if let Some(fields) = doc.fields {
            let plan = fields
                .get("plan_tier")
                .and_then(|v| String::from_firestore_value(v))
                .or_else(|| fields.get("plan").and_then(|v| String::from_firestore_value(v)));

            if let Some(plan) = plan {
                tier = PlanTier::from_str(&plan);
            }
        }
    }

    let limit_bytes = tier.storage_limit_bytes();
    let repo = StorageAccountingRepository::new(ctx.firestore.clone(), user_id);
    match repo
        .would_exceed_quota(ESTIMATED_CLIP_SIZE_BYTES, limit_bytes)
        .await
    {
        Ok(true) => Err(WorkerError::quota_exceeded(format!(
            "Storage quota exceeded (plan: {}, limit_bytes: {}, estimated_clip_bytes: {})",
            tier.as_str(),
            limit_bytes,
            ESTIMATED_CLIP_SIZE_BYTES
        ))),
        Ok(false) => Ok(()),
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to enforce quota (allowing clip)");
            Ok(())
        }
    }
}
