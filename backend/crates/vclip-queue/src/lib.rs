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
pub use job::{ProcessVideoJob, QueueJob, RenderSceneStyleJob, ReprocessScenesJob};
pub use progress::{ProgressChannel, ProgressEvent};
pub use queue::{JobQueue, QueueConfig};
