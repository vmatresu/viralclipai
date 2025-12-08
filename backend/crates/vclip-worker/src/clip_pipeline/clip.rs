use std::path::Path;

use tracing::info;
use vclip_firestore::ClipRepository;
use vclip_media::core::{
    ProcessingContext as MediaProcessingContext, ProcessingRequest,
};
use vclip_media::intelligent::parse_timestamp;
use vclip_models::{
    ClipMetadata, ClipProcessingStep, ClipTask, EncodingConfig, JobId, Style, VideoId,
};

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;

/// Select encoding configuration per style.
fn encoding_for_style(style: Style) -> EncodingConfig {
    match style {
        Style::Intelligent
        | Style::IntelligentBasic
        | Style::IntelligentAudio
        | Style::IntelligentSpeaker
        | Style::IntelligentMotion
        | Style::IntelligentActivity
        | Style::IntelligentSplit
        | Style::IntelligentSplitBasic
        | Style::IntelligentSplitAudio
        | Style::IntelligentSplitSpeaker
        | Style::IntelligentSplitMotion
        | Style::IntelligentSplitActivity => EncodingConfig::for_intelligent_crop(),
        // Static Split/Focus styles use higher CRF to shrink output size.
        Style::Split => EncodingConfig::for_split_view().with_crf(24),
        Style::SplitFast => EncodingConfig::for_split_view().with_crf(24),
        Style::LeftFocus => EncodingConfig::for_split_view().with_crf(24),
        Style::RightFocus => EncodingConfig::for_split_view().with_crf(24),
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
        Some(format!("{:.1}s - {:.1}s ({:.1}s)", start_sec, end_sec, duration_sec))
    );

    // Create processing request with error context
    let request = ProcessingRequest::new(
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
        Some(format!("Style: {}", style_name))
    );

    // Get style processor and process (strategy pattern)
    let processor = ctx.style_registry.get_processor(task.style).await.map_err(|e| {
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
        .clip_uploaded(job_id, video_id.as_str(), clip_index as u32 + 1, total_clips as u32)
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
        status: vclip_models::ClipStatus::Completed,
        created_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        created_by: user_id.to_string(),
    };

    // Persist clip metadata to Firestore (repository pattern)
    let clip_repo = ClipRepository::new(ctx.firestore.clone(), user_id, video_id.clone());

    if let Err(e) = clip_repo.create(&clip_meta).await {
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            clip_id = %clip_meta.clip_id,
            error = %e,
            "Failed to save clip metadata to Firestore (non-critical)"
        );
        // Note: We don't fail the job if metadata save fails
        // The clip is already uploaded and usable
    }

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

