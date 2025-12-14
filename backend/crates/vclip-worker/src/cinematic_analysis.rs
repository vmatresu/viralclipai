//! Cinematic analysis status management.
//!
//! Provides helpers for managing the cinematic analysis-first pattern
//! where render jobs must wait for analysis to complete before processing.

use redis::AsyncCommands;
use tracing::{debug, info, warn};
use vclip_models::{
    CinematicAnalysisStatus, DetectionTier, cinematic_analysis_key,
    CINEMATIC_ANALYSIS_TIMEOUT_SECS,
};

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;

/// Get the current cinematic analysis status from Redis.
pub async fn get_analysis_status(
    ctx: &EnhancedProcessingContext,
    video_id: &str,
    scene_id: u32,
) -> WorkerResult<CinematicAnalysisStatus> {
    let key = cinematic_analysis_key(video_id, scene_id);
    
    let mut conn = ctx
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| WorkerError::queue_failed(format!("Redis connection failed: {}", e)))?;
    
    let status_json: Option<String> = conn
        .get(&key)
        .await
        .map_err(|e| WorkerError::queue_failed(format!("Redis GET failed: {}", e)))?;
    
    match status_json {
        Some(json) => {
            let status: CinematicAnalysisStatus = serde_json::from_str(&json)
                .map_err(|e| WorkerError::queue_failed(format!("Failed to parse status: {}", e)))?;
            Ok(status)
        }
        None => Ok(CinematicAnalysisStatus::NotStarted),
    }
}

/// Set the cinematic analysis status in Redis.
pub async fn set_analysis_status(
    ctx: &EnhancedProcessingContext,
    video_id: &str,
    scene_id: u32,
    status: &CinematicAnalysisStatus,
) -> WorkerResult<()> {
    let key = cinematic_analysis_key(video_id, scene_id);
    
    let json = serde_json::to_string(status)
        .map_err(|e| WorkerError::queue_failed(format!("Failed to serialize status: {}", e)))?;
    
    let mut conn = ctx
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| WorkerError::queue_failed(format!("Redis connection failed: {}", e)))?;
    
    // Set with expiry (slightly longer than timeout to allow for processing)
    let expiry_secs = CINEMATIC_ANALYSIS_TIMEOUT_SECS + 3600; // 25 hours
    
    conn.set_ex::<_, _, ()>(&key, json, expiry_secs)
        .await
        .map_err(|e| WorkerError::queue_failed(format!("Redis SET failed: {}", e)))?;
    
    debug!(
        video_id = video_id,
        scene_id = scene_id,
        "Set cinematic analysis status"
    );
    
    Ok(())
}

/// Check if the style requires analysis-first processing (Cinematic tier only).
pub fn requires_analysis_first(style: &vclip_models::Style) -> bool {
    style.detection_tier() == DetectionTier::Cinematic
}

/// Handle cinematic analysis-first pattern for a render job.
///
/// Returns:
/// - `Ok(true)` if processing should proceed (analysis is complete)
/// - `Ok(false)` if job was rescheduled (analysis pending)
/// - `Err(...)` if analysis failed or timed out
pub async fn check_or_queue_analysis(
    ctx: &EnhancedProcessingContext,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
) -> WorkerResult<bool> {
    let status = get_analysis_status(ctx, video_id, scene_id).await?;
    
    match status {
        CinematicAnalysisStatus::Complete { .. } => {
            info!(
                video_id = video_id,
                scene_id = scene_id,
                "Cinematic analysis complete, proceeding with processing"
            );
            Ok(true)
        }
        
        CinematicAnalysisStatus::InProgress { started_at } => {
            // Check for timeout
            if status.is_timed_out() {
                warn!(
                    video_id = video_id,
                    scene_id = scene_id,
                    started_at = %started_at,
                    "Cinematic analysis timed out after 24 hours"
                );
                // Mark as failed to clean up
                let failed = CinematicAnalysisStatus::failed("Analysis timed out after 24 hours");
                set_analysis_status(ctx, video_id, scene_id, &failed).await?;
                
                return Err(WorkerError::job_failed(
                    "Cinematic analysis timed out. Please retry the job."
                ));
            }
            
            info!(
                video_id = video_id,
                scene_id = scene_id,
                started_at = %started_at,
                "Cinematic analysis in progress, should reschedule"
            );
            
            // Job should be rescheduled by caller
            Ok(false)
        }
        
        CinematicAnalysisStatus::NotStarted => {
            info!(
                video_id = video_id,
                scene_id = scene_id,
                "Cinematic analysis not started, queuing analysis job"
            );
            
            // Mark as in progress
            let in_progress = CinematicAnalysisStatus::in_progress();
            set_analysis_status(ctx, video_id, scene_id, &in_progress).await?;
            
            // Queue analysis job (neural analysis with Cinematic tier)
            queue_cinematic_analysis_job(ctx, user_id, video_id, scene_id).await?;
            
            // Job should be rescheduled by caller
            Ok(false)
        }
        
        CinematicAnalysisStatus::Failed { error, failed_at } => {
            warn!(
                video_id = video_id,
                scene_id = scene_id,
                error = %error,
                failed_at = %failed_at,
                "Cinematic analysis previously failed"
            );
            Err(WorkerError::job_failed(format!(
                "Cinematic analysis failed: {}",
                error
            )))
        }
    }
}

/// Queue a cinematic analysis job.
async fn queue_cinematic_analysis_job(
    ctx: &EnhancedProcessingContext,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
) -> WorkerResult<()> {
    let Some(ref queue) = ctx.job_queue else {
        warn!("No job queue available, cannot queue cinematic analysis job");
        return Err(WorkerError::job_failed(
            "Job queue not available for cinematic analysis"
        ));
    };
    
    // Check Firestore for source video R2 key hint
    let video_id_typed = vclip_models::VideoId::from(video_id.to_string());
    let source_hint = match vclip_firestore::VideoRepository::new(ctx.firestore.clone(), user_id)
        .get(&video_id_typed)
        .await
    {
        Ok(Some(video_meta)) => video_meta.source_video_r2_key,
        _ => None,
    };
    
    let neural_job = vclip_queue::NeuralAnalysisJob {
        job_id: vclip_models::JobId::new(),
        user_id: user_id.to_string(),
        video_id: vclip_models::VideoId::from(video_id.to_string()),
        scene_id,
        source_hint_r2_key: source_hint,
        detection_tier: DetectionTier::Cinematic,
        created_at: chrono::Utc::now(),
    };
    
    match queue.enqueue_neural_analysis(neural_job).await {
        Ok(msg_id) => {
            info!(
                video_id = video_id,
                scene_id = scene_id,
                message_id = %msg_id,
                "Enqueued cinematic analysis job"
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                video_id = video_id,
                scene_id = scene_id,
                error = %e,
                "Failed to enqueue cinematic analysis job"
            );
            Err(WorkerError::queue_failed(format!(
                "Failed to enqueue cinematic analysis: {}",
                e
            )))
        }
    }
}

/// Mark cinematic analysis as complete.
pub async fn mark_analysis_complete(
    ctx: &EnhancedProcessingContext,
    video_id: &str,
    scene_id: u32,
) -> WorkerResult<()> {
    let status = CinematicAnalysisStatus::complete();
    set_analysis_status(ctx, video_id, scene_id, &status).await
}

/// Mark cinematic analysis as failed.
pub async fn mark_analysis_failed(
    ctx: &EnhancedProcessingContext,
    video_id: &str,
    scene_id: u32,
    error: impl Into<String>,
) -> WorkerResult<()> {
    let status = CinematicAnalysisStatus::failed(error);
    set_analysis_status(ctx, video_id, scene_id, &status).await
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_requires_analysis_first() {
        use vclip_models::Style;
        
        // Cinematic requires analysis first
        assert!(requires_analysis_first(&Style::IntelligentCinematic));
        
        // Other styles do not
        assert!(!requires_analysis_first(&Style::Intelligent));
        assert!(!requires_analysis_first(&Style::IntelligentSpeaker));
        assert!(!requires_analysis_first(&Style::Split));
    }
}
