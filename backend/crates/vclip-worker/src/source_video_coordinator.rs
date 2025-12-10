//! Source video lifecycle coordinator.
//!
//! Manages the lifecycle of source video files across distributed workers
//! using Redis-based reference counting. This ensures:
//! - Video is downloaded once (first worker to arrive)
//! - Video is not deleted while other workers still need it
//! - Video is cleaned up when the last worker finishes

use redis::AsyncCommands;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Default TTL for active job keys (24 hours).
/// This provides crash recovery - if a worker dies, the key will eventually expire.
const DEFAULT_KEY_TTL_SECS: u64 = 24 * 60 * 60;

/// Coordinator for managing source video lifecycle across workers.
pub struct SourceVideoCoordinator {
    client: redis::Client,
    key_ttl: Duration,
}

impl SourceVideoCoordinator {
    /// Create a new coordinator.
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            key_ttl: Duration::from_secs(DEFAULT_KEY_TTL_SECS),
        })
    }

    /// Create with custom TTL for testing.
    #[allow(dead_code)]
    pub fn with_ttl(redis_url: &str, ttl: Duration) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            key_ttl: ttl,
        })
    }

    /// Get the Redis key for tracking active jobs on a video.
    fn active_jobs_key(user_id: &str, video_id: &str) -> String {
        format!("video:{}:{}:active_jobs", user_id, video_id)
    }

    /// Called when a job starts processing a video.
    ///
    /// Atomically increments the active job count and sets/refreshes the TTL.
    /// Returns the new active job count.
    pub async fn job_started(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::active_jobs_key(user_id, video_id);

        // Atomically increment and set TTL
        let count: i64 = conn.incr(&key, 1).await?;
        let _: () = conn.expire(&key, self.key_ttl.as_secs() as i64).await?;

        debug!(
            video_id = video_id,
            active_jobs = count,
            "Incremented active jobs for video"
        );

        Ok(count)
    }

    /// Called when a job finishes processing (success or failure).
    ///
    /// Atomically decrements the active job count.
    /// Returns true if this was the last job (count reached 0) and cleanup should occur.
    pub async fn job_finished(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> Result<bool, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::active_jobs_key(user_id, video_id);

        // Atomically decrement
        let remaining: i64 = conn.decr(&key, 1).await?;

        debug!(
            video_id = video_id,
            remaining_jobs = remaining,
            "Decremented active jobs for video"
        );

        if remaining <= 0 {
            // Clean up the key
            let _: () = conn.del(&key).await?;
            info!(video_id = video_id, "Last job complete, cleanup authorized");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Cleanup the work directory for a video.
    ///
    /// This should be called when `job_finished` returns true.
    pub async fn cleanup_work_dir(work_dir: &Path) -> Result<(), std::io::Error> {
        if work_dir.exists() {
            info!("Cleaning up work directory: {}", work_dir.display());
            tokio::fs::remove_dir_all(work_dir).await?;
        }
        Ok(())
    }

    /// Get the current active job count for a video.
    ///
    /// Useful for debugging and monitoring.
    #[allow(dead_code)]
    pub async fn get_active_count(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::active_jobs_key(user_id, video_id);
        let count: Option<i64> = conn.get(&key).await?;
        Ok(count.unwrap_or(0))
    }

    /// Force cleanup a stale tracking key.
    ///
    /// This can be used by an admin/cleanup job to handle orphaned keys
    /// from crashed workers.
    #[allow(dead_code)]
    pub async fn force_cleanup(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::active_jobs_key(user_id, video_id);

        warn!(video_id = video_id, "Force cleaning up active jobs key");

        let _: () = conn.del(&key).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Redis instance.
    // They are marked as ignored by default.

    #[tokio::test]
    #[ignore]
    async fn test_job_lifecycle() {
        let coordinator = SourceVideoCoordinator::new("redis://localhost:6379").unwrap();
        let user_id = "test_user";
        let video_id = "test_video_lifecycle";

        // Start first job
        let count = coordinator.job_started(user_id, video_id).await.unwrap();
        assert_eq!(count, 1);

        // Start second job
        let count = coordinator.job_started(user_id, video_id).await.unwrap();
        assert_eq!(count, 2);

        // Finish first job - should not cleanup
        let should_cleanup = coordinator.job_finished(user_id, video_id).await.unwrap();
        assert!(!should_cleanup);

        // Finish second job - should cleanup
        let should_cleanup = coordinator.job_finished(user_id, video_id).await.unwrap();
        assert!(should_cleanup);

        // Verify count is 0
        let count = coordinator
            .get_active_count(user_id, video_id)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
