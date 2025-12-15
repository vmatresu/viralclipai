//! Background service for detecting and recovering stale jobs.
//!
//! This service runs periodically to:
//! - Detect jobs that have stopped responding (no heartbeat)
//! - Mark them as failed in both Redis and Firebase
//! - Notify any connected clients via progress channel
//! - Clean up orphaned entries from the active jobs set

use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tracing::{error, info, warn};

use vclip_firestore::{FirestoreClient, VideoRepository};
use vclip_models::{JobId, JobStatus, JobStatusCache, VideoId, VideoStatus};
use vclip_queue::{ProgressChannel, STALE_GRACE_PERIOD_SECS, STALE_THRESHOLD_SECS};

/// Interval between stale job detection runs.
const DETECTION_INTERVAL: Duration = Duration::from_secs(30);

/// Stale job detector service.
pub struct StaleJobDetector {
    progress: Arc<ProgressChannel>,
    firestore: Arc<FirestoreClient>,
    enabled: bool,
}

impl StaleJobDetector {
    /// Create a new stale job detector.
    pub fn new(progress: Arc<ProgressChannel>, firestore: Arc<FirestoreClient>) -> Self {
        // Check if stale detection is enabled via environment variable
        let enabled = std::env::var("ENABLE_STALE_DETECTION")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true); // Enabled by default

        Self {
            progress,
            firestore,
            enabled,
        }
    }

    /// Start the background detection loop.
    ///
    /// This function runs indefinitely and should be spawned as a background task.
    pub async fn run(&self) {
        if !self.enabled {
            info!("Stale job detection is disabled");
            return;
        }

        info!("Starting stale job detector (interval: {:?})", DETECTION_INTERVAL);

        let mut ticker = interval(DETECTION_INTERVAL);

        loop {
            ticker.tick().await;

            if let Err(e) = self.detect_and_recover().await {
                error!("Stale job detection error: {}", e);
            }
        }
    }

    /// Run a single detection and recovery cycle.
    async fn detect_and_recover(&self) -> anyhow::Result<()> {
        // Get all active jobs from Redis
        let active_jobs = self.progress.get_active_jobs().await?;

        if active_jobs.is_empty() {
            return Ok(());
        }

        let mut stale_count = 0u32;
        let mut recovered_count = 0u32;

        for job_status in active_jobs {
            // Skip terminal states
            if job_status.is_terminal() {
                // Clean up - this job shouldn't be in active set
                self.progress
                    .remove_from_active_jobs_by_id(&job_status.job_id)
                    .await
                    .ok();
                continue;
            }

            // Check if job is stale
            let is_stale = job_status.is_stale(STALE_THRESHOLD_SECS, STALE_GRACE_PERIOD_SECS);

            if is_stale {
                stale_count += 1;

                warn!(
                    job_id = %job_status.job_id,
                    video_id = %job_status.video_id,
                    user_id = %job_status.user_id,
                    last_heartbeat = ?job_status.last_heartbeat,
                    started_at = %job_status.started_at,
                    "Detected stale job (no heartbeat)"
                );

                // Recover the job
                if let Err(e) = self.recover_stale_job(&job_status).await {
                    error!(
                        job_id = %job_status.job_id,
                        "Failed to recover stale job: {}", e
                    );
                } else {
                    recovered_count += 1;
                    info!(
                        job_id = %job_status.job_id,
                        video_id = %job_status.video_id,
                        "Successfully recovered stale job"
                    );
                }
            }
        }

        if stale_count > 0 {
            info!(
                "Stale job detection complete: {} stale, {} recovered",
                stale_count, recovered_count
            );
        }

        // Periodic cleanup of orphaned active jobs
        let cleaned = self.progress.cleanup_active_jobs().await?;
        if cleaned > 0 {
            info!("Cleaned up {} orphaned active job entries", cleaned);
        }

        Ok(())
    }

    /// Recover a stale job by marking it as failed.
    async fn recover_stale_job(
        &self,
        job_status: &JobStatusCache,
    ) -> anyhow::Result<()> {
        let job_id = JobId::from(job_status.job_id.clone());
        let error_message = "Processing timed out. The worker may have crashed. Please try again.";

        // 1. Update job status cache in Redis
        let mut updated_status = job_status.clone();
        updated_status.status = JobStatus::Failed;
        updated_status.error_message = Some(error_message.to_string());
        updated_status.updated_at = chrono::Utc::now();
        updated_status.event_seq += 1;

        self.progress
            .update_job_status(&job_id, &updated_status)
            .await?;

        // 2. Publish error event so any connected clients get notified
        self.progress.error(&job_id, error_message).await.ok();

        // 3. Update Firebase video status
        let video_repo = VideoRepository::new(
            (*self.firestore).clone(),
            &job_status.user_id,
        );

        let video_id = VideoId::from_string(&job_status.video_id);
        if let Err(e) = video_repo.update_status(&video_id, VideoStatus::Failed).await {
            error!(
                video_id = %job_status.video_id,
                "Failed to update Firebase status: {}", e
            );
            // Don't fail the recovery - Redis is updated, client will see the failure
        }

        // 4. Remove from active jobs set
        self.progress.remove_from_active_jobs_by_id(&job_status.job_id).await?;

        // 5. Clear heartbeat key
        self.progress.clear_heartbeat(&job_id).await?;

        Ok(())
    }

    /// Run a single check (for testing or manual invocation).
    pub async fn check_once(&self) -> anyhow::Result<(u32, u32)> {
        let active_jobs = self.progress.get_active_jobs().await?;
        let mut stale_count = 0u32;
        let mut recovered_count = 0u32;

        for job_status in active_jobs {
            if job_status.is_terminal() {
                continue;
            }

            let is_stale = job_status.is_stale(STALE_THRESHOLD_SECS, STALE_GRACE_PERIOD_SECS);

            if is_stale {
                stale_count += 1;
                if self.recover_stale_job(&job_status).await.is_ok() {
                    recovered_count += 1;
                }
            }
        }

        Ok((stale_count, recovered_count))
    }
}
