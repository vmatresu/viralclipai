//! Job status cache for progress tracking and polling.
//!
//! This module provides types for caching job status in Redis,
//! enabling fast polling queries and stale job detection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cached job status for fast polling queries.
///
/// This is stored in Redis and provides a snapshot of the current
/// job state without needing to query Firestore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusCache {
    /// Unique job identifier
    pub job_id: String,
    /// Associated video ID
    pub video_id: String,
    /// User who owns this job
    pub user_id: String,
    /// Current job status
    pub status: JobStatus,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Number of clips completed
    pub clips_completed: u32,
    /// Total number of clips to process
    pub clips_total: u32,
    /// Current processing step description
    pub current_step: Option<String>,
    /// Error message if job failed
    pub error_message: Option<String>,
    /// When the job was started
    pub started_at: DateTime<Utc>,
    /// When the status was last updated
    pub updated_at: DateTime<Utc>,
    /// Last heartbeat from worker
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Sequence number for event ordering (monotonically increasing)
    pub event_seq: u64,
}

/// Job processing status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is queued waiting for a worker
    #[default]
    Queued,
    /// Job is actively being processed
    Processing,
    /// Job completed successfully
    Completed,
    /// Job failed with an error
    Failed,
    /// Worker stopped responding (stale)
    Stale,
}

impl JobStatus {
    /// Get string representation of the status.
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Processing => "processing",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Stale => "stale",
        }
    }

    /// Check if this is a terminal state (no more updates expected).
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed)
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl JobStatusCache {
    /// Create a new job status cache entry.
    pub fn new(job_id: impl Into<String>, video_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            job_id: job_id.into(),
            video_id: video_id.into(),
            user_id: user_id.into(),
            status: JobStatus::Queued,
            progress: 0,
            clips_completed: 0,
            clips_total: 0,
            current_step: None,
            error_message: None,
            started_at: now,
            updated_at: now,
            last_heartbeat: None,
            event_seq: 0,
        }
    }

    /// Check if the job is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Update the status and bump the updated_at timestamp.
    pub fn set_status(&mut self, status: JobStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Update progress and bump event sequence.
    pub fn set_progress(&mut self, progress: u8) {
        self.progress = progress.min(100);
        self.updated_at = Utc::now();
        self.event_seq += 1;
    }

    /// Update heartbeat timestamp.
    pub fn record_heartbeat(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark job as completed.
    pub fn complete(&mut self) {
        self.status = JobStatus::Completed;
        self.progress = 100;
        self.current_step = Some("Complete".into());
        self.updated_at = Utc::now();
        self.event_seq += 1;
    }

    /// Mark job as failed with an error message.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = JobStatus::Failed;
        self.error_message = Some(error.into());
        self.updated_at = Utc::now();
        self.event_seq += 1;
    }

    /// Mark job as stale (worker timeout).
    pub fn mark_stale(&mut self) {
        self.status = JobStatus::Stale;
        self.error_message = Some("Processing timed out. The worker may have crashed. Please try again.".into());
        self.updated_at = Utc::now();
        self.event_seq += 1;
    }

    /// Check if the job should be considered stale based on heartbeat.
    ///
    /// A job is stale if:
    /// - It's not in a terminal state
    /// - Either no heartbeat received and job is older than grace_period_secs
    /// - Or last heartbeat is older than stale_threshold_secs
    pub fn is_stale(&self, stale_threshold_secs: i64, grace_period_secs: i64) -> bool {
        if self.is_terminal() {
            return false;
        }

        let now = Utc::now();
        match self.last_heartbeat {
            Some(hb) => (now - hb).num_seconds() > stale_threshold_secs,
            None => (now - self.started_at).num_seconds() > grace_period_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_cache_creation() {
        let cache = JobStatusCache::new("job-1", "video-1", "user-1");
        assert_eq!(cache.status, JobStatus::Queued);
        assert_eq!(cache.progress, 0);
        assert!(!cache.is_terminal());
    }

    #[test]
    fn test_job_status_transitions() {
        let mut cache = JobStatusCache::new("job-1", "video-1", "user-1");

        cache.set_status(JobStatus::Processing);
        assert_eq!(cache.status, JobStatus::Processing);
        assert!(!cache.is_terminal());

        cache.set_progress(50);
        assert_eq!(cache.progress, 50);

        cache.complete();
        assert_eq!(cache.status, JobStatus::Completed);
        assert_eq!(cache.progress, 100);
        assert!(cache.is_terminal());
    }

    #[test]
    fn test_job_status_stale_detection() {
        let mut cache = JobStatusCache::new("job-1", "video-1", "user-1");
        cache.set_status(JobStatus::Processing);

        // Within grace period, not stale
        assert!(!cache.is_stale(60, 120));

        // Simulate old job without heartbeat
        cache.started_at = Utc::now() - chrono::Duration::seconds(200);
        assert!(cache.is_stale(60, 120));

        // With recent heartbeat, not stale
        cache.record_heartbeat();
        assert!(!cache.is_stale(60, 120));
    }
}
