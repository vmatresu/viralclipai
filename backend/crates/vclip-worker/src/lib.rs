//! Video processing worker.
//!
//! This crate provides:
//! - Job executor for process/reprocess jobs
//! - Clip rendering and upload
//! - Progress emission
//! - Graceful shutdown
//! - New modular architecture with security and performance

pub mod config;
pub mod error;
pub mod executor;
pub mod gemini;
pub mod clip_pipeline;
pub mod processor;
pub mod reprocessing;

pub use config::WorkerConfig;
pub use error::{WorkerError, WorkerResult};
pub use executor::JobExecutor;
pub use processor::{VideoProcessor, EnhancedProcessingContext};
