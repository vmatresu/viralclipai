//! Progress events via Redis Pub/Sub with persistence and heartbeat support.
//!
//! This module provides:
//! - Real-time progress events via Redis Pub/Sub
//! - Persistent progress history via Redis Sorted Sets
//! - Worker heartbeat tracking for stale job detection
//! - Job status caching for fast polling

use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use vclip_models::{ClipProcessingStep, JobId, JobStatus, JobStatusCache, WsMessage};

use crate::error::QueueResult;

// ============================================================================
// Redis Key Prefixes and TTLs
// ============================================================================

/// Prefix for worker heartbeat keys: `heartbeat:{job_id}`
const HEARTBEAT_KEY_PREFIX: &str = "heartbeat:";

/// Prefix for progress history sorted sets: `progress:history:{job_id}`
const PROGRESS_HISTORY_PREFIX: &str = "progress:history:";

/// Prefix for job status cache: `job:status:{job_id}`
const JOB_STATUS_PREFIX: &str = "job:status:";

/// Prefix for active jobs set: `jobs:active`
const ACTIVE_JOBS_KEY: &str = "jobs:active";

/// Heartbeat TTL - job considered dead after this duration without heartbeat (seconds)
pub const HEARTBEAT_TTL_SECS: u64 = 60;

/// Progress history TTL - keep progress events for recovery (seconds)
pub const PROGRESS_HISTORY_TTL_SECS: u64 = 3600; // 1 hour

/// Job status cache TTL (seconds)
pub const JOB_STATUS_TTL_SECS: u64 = 86400; // 24 hours

/// Grace period before marking a job without heartbeat as stale (seconds)
pub const STALE_GRACE_PERIOD_SECS: i64 = 120;

/// Stale threshold - no heartbeat for this long means stale (seconds)
pub const STALE_THRESHOLD_SECS: i64 = 60;

// ============================================================================
// Data Structures
// ============================================================================

/// Progress event published to Redis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Job ID
    pub job_id: JobId,
    /// WebSocket message
    pub message: WsMessage,
    /// Event timestamp (milliseconds since epoch)
    #[serde(default = "default_timestamp")]
    pub timestamp_ms: i64,
    /// Sequence number for ordering
    #[serde(default)]
    pub seq: u64,
}

fn default_timestamp() -> i64 {
    Utc::now().timestamp_millis()
}

impl ProgressEvent {
    /// Create a new progress event with current timestamp.
    pub fn new(job_id: JobId, message: WsMessage) -> Self {
        Self {
            job_id,
            message,
            timestamp_ms: Utc::now().timestamp_millis(),
            seq: 0,
        }
    }

    /// Set the sequence number.
    pub fn with_seq(mut self, seq: u64) -> Self {
        self.seq = seq;
        self
    }
}

/// Channel for publishing/subscribing to progress events.
#[derive(Clone)]
pub struct ProgressChannel {
    client: redis::Client,
}

impl ProgressChannel {
    /// Create a new progress channel.
    pub fn new(redis_url: &str) -> QueueResult<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }

    /// Get the channel name for a job.
    pub fn channel_name(job_id: &JobId) -> String {
        format!("progress:{}", job_id)
    }

    /// Publish a progress event (Pub/Sub only, no persistence).
    ///
    /// For most use cases, prefer `publish_with_history` which also persists
    /// the event for recovery purposes.
    pub async fn publish(&self, event: &ProgressEvent) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let channel = Self::channel_name(&event.job_id);
        let payload = serde_json::to_string(event)?;

        debug!("Publishing progress event to {}", channel);
        conn.publish::<_, _, ()>(channel, payload).await?;

        Ok(())
    }

    /// Publish a progress event with persistence to history.
    ///
    /// This performs a dual-write:
    /// 1. Pub/Sub for real-time delivery to connected clients
    /// 2. Sorted set for history/recovery (scored by timestamp)
    pub async fn publish_with_history(&self, event: &ProgressEvent) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let channel = Self::channel_name(&event.job_id);
        let history_key = format!("{}{}", PROGRESS_HISTORY_PREFIX, event.job_id);
        let payload = serde_json::to_string(event)?;
        let score = event.timestamp_ms as f64;

        debug!("Publishing progress event to {} with history", channel);

        // Dual-write: Pub/Sub + Sorted Set
        redis::pipe()
            .publish(&channel, &payload)
            .ignore()
            .zadd(&history_key, &payload, score)
            .ignore()
            .expire(&history_key, PROGRESS_HISTORY_TTL_SECS as i64)
            .ignore()
            .exec_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Publish a log message.
    pub async fn log(&self, job_id: &JobId, message: impl Into<String>) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(job_id.clone(), WsMessage::log(message)))
            .await
    }

    /// Publish a progress update.
    pub async fn progress(&self, job_id: &JobId, value: u8) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(job_id.clone(), WsMessage::progress(value)))
            .await
    }

    /// Publish a clip uploaded notification.
    pub async fn clip_uploaded(
        &self,
        job_id: &JobId,
        video_id: &str,
        clip_count: u32,
        total_clips: u32,
        credits: u32,
    ) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(
            job_id.clone(),
            WsMessage::clip_uploaded_with_credits(video_id, clip_count, total_clips, credits),
        ))
        .await
    }

    /// Publish done message.
    pub async fn done(&self, job_id: &JobId, video_id: &str) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(job_id.clone(), WsMessage::done(video_id)))
            .await
    }

    /// Publish error message.
    pub async fn error(&self, job_id: &JobId, message: impl Into<String>) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(job_id.clone(), WsMessage::error(message)))
            .await
    }

    /// Publish clip progress message.
    pub async fn clip_progress(
        &self,
        job_id: &JobId,
        scene_id: u32,
        style: &str,
        step: ClipProcessingStep,
        details: Option<String>,
    ) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(
            job_id.clone(),
            WsMessage::clip_progress(scene_id, style, step, details),
        ))
        .await
    }

    /// Publish scene started message.
    pub async fn scene_started(
        &self,
        job_id: &JobId,
        scene_id: u32,
        scene_title: &str,
        style_count: u32,
        start_sec: f64,
        duration_sec: f64,
    ) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(
            job_id.clone(),
            WsMessage::scene_started(scene_id, scene_title, style_count, start_sec, duration_sec),
        ))
        .await
    }

    /// Publish scene completed message.
    pub async fn scene_completed(
        &self,
        job_id: &JobId,
        scene_id: u32,
        clips_completed: u32,
        clips_failed: u32,
    ) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(
            job_id.clone(),
            WsMessage::scene_completed(scene_id, clips_completed, clips_failed),
        ))
        .await
    }

    /// Publish style omitted notification.
    ///
    /// Used when a style is skipped because it would produce identical output
    /// to another style (e.g., split style falling back to full-frame).
    pub async fn style_omitted(
        &self,
        job_id: &JobId,
        scene_id: u32,
        style: &str,
        reason: &str,
    ) -> QueueResult<()> {
        self.publish_with_history(&ProgressEvent::new(
            job_id.clone(),
            WsMessage::style_omitted(scene_id, style, reason),
        ))
        .await
    }

    /// Subscribe to progress events for a job.
    /// Returns a pinned stream that can be polled with `.next()`.
    pub async fn subscribe(
        &self,
        job_id: &JobId,
    ) -> QueueResult<std::pin::Pin<Box<dyn futures_util::Stream<Item = ProgressEvent> + Send>>> {
        use futures_util::StreamExt;

        let mut pubsub = self.client.get_async_pubsub().await?;
        let channel = Self::channel_name(job_id);

        pubsub.subscribe(&channel).await?;

        let stream = pubsub.into_on_message().filter_map(|msg| async move {
            let payload: String = msg.get_payload().ok()?;
            serde_json::from_str(&payload).ok()
        });

        Ok(Box::pin(stream))
    }

    // ========================================================================
    // Heartbeat Methods
    // ========================================================================

    /// Update worker heartbeat for a job.
    ///
    /// Workers should call this every 10 seconds during processing.
    /// The heartbeat key has a 60-second TTL, so missing 6 consecutive
    /// heartbeats will cause the job to be considered stale.
    pub async fn heartbeat(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);
        let now = Utc::now().timestamp();

        conn.set_ex::<_, _, ()>(&key, now, HEARTBEAT_TTL_SECS).await?;
        debug!("Updated heartbeat for job {}", job_id);

        Ok(())
    }

    /// Check if a job has an active heartbeat.
    pub async fn is_alive(&self, job_id: &JobId) -> QueueResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);

        let exists: bool = conn.exists(&key).await?;
        Ok(exists)
    }

    /// Get the last heartbeat timestamp for a job.
    pub async fn get_last_heartbeat(&self, job_id: &JobId) -> QueueResult<Option<i64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);

        let timestamp: Option<i64> = conn.get(&key).await?;
        Ok(timestamp)
    }

    /// Clear heartbeat when job completes.
    pub async fn clear_heartbeat(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);

        conn.del::<_, ()>(&key).await?;
        Ok(())
    }

    // ========================================================================
    // Progress History Methods
    // ========================================================================

    /// Get progress history since a given timestamp.
    ///
    /// Returns all progress events with timestamp >= since_ms.
    pub async fn get_history_since(
        &self,
        job_id: &JobId,
        since_ms: i64,
    ) -> QueueResult<Vec<ProgressEvent>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", PROGRESS_HISTORY_PREFIX, job_id);

        let events: Vec<String> = conn
            .zrangebyscore(&key, since_ms as f64, "+inf")
            .await?;

        let parsed: Vec<ProgressEvent> = events
            .into_iter()
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();

        Ok(parsed)
    }

    /// Get all progress history for a job.
    pub async fn get_full_history(&self, job_id: &JobId) -> QueueResult<Vec<ProgressEvent>> {
        self.get_history_since(job_id, 0).await
    }

    /// Get the count of progress events for a job.
    pub async fn get_history_count(&self, job_id: &JobId) -> QueueResult<u64> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", PROGRESS_HISTORY_PREFIX, job_id);

        let count: u64 = conn.zcard(&key).await?;
        Ok(count)
    }

    /// Clear progress history for a job.
    pub async fn clear_history(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", PROGRESS_HISTORY_PREFIX, job_id);

        conn.del::<_, ()>(&key).await?;
        Ok(())
    }

    // ========================================================================
    // Job Status Cache Methods
    // ========================================================================

    /// Initialize job status cache when a job starts.
    pub async fn init_job_status(
        &self,
        job_id: &JobId,
        video_id: &str,
        user_id: &str,
        total_clips: u32,
    ) -> QueueResult<()> {
        let mut status = JobStatusCache::new(job_id.to_string(), video_id, user_id);
        status.clips_total = total_clips;
        status.set_status(JobStatus::Processing);

        self.update_job_status(job_id, &status).await?;
        self.add_to_active_jobs(job_id).await?;

        Ok(())
    }

    /// Update job status cache.
    pub async fn update_job_status(
        &self,
        job_id: &JobId,
        status: &JobStatusCache,
    ) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", JOB_STATUS_PREFIX, job_id);
        let payload = serde_json::to_string(status)?;

        conn.set_ex::<_, _, ()>(&key, payload, JOB_STATUS_TTL_SECS).await?;
        Ok(())
    }

    /// Get cached job status.
    pub async fn get_job_status(&self, job_id: &JobId) -> QueueResult<Option<JobStatusCache>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", JOB_STATUS_PREFIX, job_id);

        let value: Option<String> = conn.get(&key).await?;
        Ok(value.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Update job progress in status cache.
    pub async fn update_job_progress(
        &self,
        job_id: &JobId,
        progress: u8,
        current_step: Option<&str>,
    ) -> QueueResult<()> {
        if let Some(mut status) = self.get_job_status(job_id).await? {
            status.set_progress(progress);
            if let Some(step) = current_step {
                status.current_step = Some(step.to_string());
            }
            self.update_job_status(job_id, &status).await?;
        }
        Ok(())
    }

    /// Update clips completed count in status cache.
    pub async fn update_clips_completed(
        &self,
        job_id: &JobId,
        clips_completed: u32,
    ) -> QueueResult<()> {
        if let Some(mut status) = self.get_job_status(job_id).await? {
            status.clips_completed = clips_completed;
            status.updated_at = Utc::now();
            status.event_seq += 1;
            self.update_job_status(job_id, &status).await?;
        }
        Ok(())
    }

    /// Mark job as completed in status cache.
    pub async fn complete_job_status(&self, job_id: &JobId) -> QueueResult<()> {
        if let Some(mut status) = self.get_job_status(job_id).await? {
            status.complete();
            self.update_job_status(job_id, &status).await?;
            self.remove_from_active_jobs(job_id).await?;
            self.clear_heartbeat(job_id).await?;
        }
        Ok(())
    }

    /// Mark job as failed in status cache.
    pub async fn fail_job_status(&self, job_id: &JobId, error: &str) -> QueueResult<()> {
        if let Some(mut status) = self.get_job_status(job_id).await? {
            status.fail(error);
            self.update_job_status(job_id, &status).await?;
            self.remove_from_active_jobs(job_id).await?;
            self.clear_heartbeat(job_id).await?;
        }
        Ok(())
    }

    // ========================================================================
    // Active Jobs Tracking
    // ========================================================================

    /// Add a job to the active jobs set.
    async fn add_to_active_jobs(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let score = Utc::now().timestamp_millis() as f64;

        conn.zadd::<_, _, _, ()>(ACTIVE_JOBS_KEY, job_id.to_string(), score).await?;
        Ok(())
    }

    /// Remove a job from the active jobs set.
    async fn remove_from_active_jobs(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        conn.zrem::<_, _, ()>(ACTIVE_JOBS_KEY, job_id.to_string()).await?;
        Ok(())
    }

    /// Remove a job from the active jobs set by string ID.
    pub async fn remove_from_active_jobs_by_id(&self, job_id: &str) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        conn.zrem::<_, _, ()>(ACTIVE_JOBS_KEY, job_id).await?;
        Ok(())
    }

    /// Get all active jobs.
    ///
    /// Used by the stale job detector to check for jobs that need recovery.
    pub async fn get_active_jobs(&self) -> QueueResult<Vec<JobStatusCache>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Get all job IDs from the active jobs set
        let job_ids: Vec<String> = conn.zrange(ACTIVE_JOBS_KEY, 0, -1).await?;

        let mut statuses = Vec::with_capacity(job_ids.len());
        for job_id in job_ids {
            if let Some(status) = self.get_job_status(&JobId::from(job_id)).await? {
                statuses.push(status);
            }
        }

        Ok(statuses)
    }

    /// Get count of active jobs.
    pub async fn get_active_job_count(&self) -> QueueResult<u64> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let count: u64 = conn.zcard(ACTIVE_JOBS_KEY).await?;
        Ok(count)
    }

    /// Clean up stale entries from active jobs set.
    ///
    /// Removes jobs that are no longer in the status cache.
    pub async fn cleanup_active_jobs(&self) -> QueueResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let job_ids: Vec<String> = conn.zrange(ACTIVE_JOBS_KEY, 0, -1).await?;

        let mut removed = 0u32;
        for job_id in job_ids {
            let key = format!("{}{}", JOB_STATUS_PREFIX, job_id);
            let exists: bool = conn.exists(&key).await?;
            if !exists {
                conn.zrem::<_, _, ()>(ACTIVE_JOBS_KEY, &job_id).await?;
                removed += 1;
                warn!("Cleaned up orphaned active job: {}", job_id);
            }
        }

        Ok(removed)
    }
}
