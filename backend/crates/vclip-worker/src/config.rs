//! Worker configuration.

use std::time::Duration;

/// Worker configuration.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Maximum concurrent jobs
    pub max_concurrent_jobs: usize,
    /// Maximum concurrent FFmpeg processes per job
    pub max_ffmpeg_processes: usize,
    /// Maximum scenes to process in parallel within a single job
    pub max_scene_parallel: usize,
    /// Maximum concurrent neural analysis operations (YuNet instances)
    /// Default: 3 to leave headroom for FFmpeg and other processes
    pub max_neural_parallel: usize,
    /// Maximum concurrent downloads per job
    pub max_download_parallel: usize,
    /// Job timeout
    pub job_timeout: Duration,
    /// Graceful shutdown timeout
    pub shutdown_timeout: Duration,
    /// Work directory for temporary files
    pub work_dir: String,
    /// How often the worker should scan for orphaned pending jobs
    pub claim_interval: Duration,
    /// Minimum idle time before a pending job can be claimed (crash recovery)
    pub claim_min_idle: Duration,
    /// Interval for refreshing job ownership while processing (prevents premature reclamation)
    pub job_heartbeat_interval: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: 2,
            max_ffmpeg_processes: 4,
            max_scene_parallel: 4, // Process up to 4 scenes in parallel within a job
            max_neural_parallel: 4, // Allow 4 concurrent neural analyses (up from 3)
            max_download_parallel: 2, // Limit concurrent downloads to avoid network saturation
            job_timeout: Duration::from_secs(3600), // 1 hour
            shutdown_timeout: Duration::from_secs(30),
            work_dir: "/tmp/vclip".to_string(),
            claim_interval: Duration::from_secs(30),
            claim_min_idle: Duration::from_secs(300), // 5 minutes
            job_heartbeat_interval: Duration::from_secs(30),
        }
    }
}

impl WorkerConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        Self {
            max_concurrent_jobs: std::env::var("WORKER_MAX_JOBS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            max_ffmpeg_processes: std::env::var("WORKER_MAX_FFMPEG")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            max_scene_parallel: std::env::var("WORKER_MAX_SCENE_PARALLEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            max_neural_parallel: std::env::var("WORKER_MAX_NEURAL_PARALLEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            max_download_parallel: std::env::var("WORKER_MAX_DOWNLOAD_PARALLEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            job_timeout: Duration::from_secs(
                std::env::var("WORKER_JOB_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3600),
            ),
            shutdown_timeout: Duration::from_secs(
                std::env::var("WORKER_SHUTDOWN_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            work_dir: std::env::var("WORKER_WORK_DIR").unwrap_or_else(|_| "/tmp/vclip".to_string()),
            claim_interval: Duration::from_secs(
                std::env::var("WORKER_CLAIM_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            claim_min_idle: Duration::from_secs(
                std::env::var("WORKER_CLAIM_MIN_IDLE_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            job_heartbeat_interval: Duration::from_secs(
                std::env::var("WORKER_JOB_HEARTBEAT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
        }
    }
}
