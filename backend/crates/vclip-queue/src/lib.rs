//! Redis Streams job queue with Apalis.
//!
//! This crate provides:
//! - Job enqueueing via Redis Streams
//! - Worker consumption with retry/DLQ
//! - Progress events via Redis Pub/Sub

pub mod error;
pub mod job;
pub mod progress;
pub mod queue;

pub use error::{QueueError, QueueResult};
pub use job::{AnalyzeVideoJob, DownloadSourceJob, NeuralAnalysisJob, ProcessVideoJob, QueueJob, RenderSceneStyleJob, ReprocessScenesJob};
pub use progress::{
    ProgressChannel, ProgressEvent,
    HEARTBEAT_TTL_SECS, PROGRESS_HISTORY_TTL_SECS, JOB_STATUS_TTL_SECS,
    STALE_GRACE_PERIOD_SECS, STALE_THRESHOLD_SECS,
};
pub use queue::{JobQueue, QueueConfig};
