//! Video processing worker.
//!
//! This crate provides:
//! - Job executor for process/reprocess jobs
//! - Clip rendering and upload
//! - Progress emission
//! - Graceful shutdown

pub mod config;
pub mod error;
pub mod executor;
pub mod processor;

pub use config::WorkerConfig;
pub use error::{WorkerError, WorkerResult};
pub use executor::JobExecutor;
