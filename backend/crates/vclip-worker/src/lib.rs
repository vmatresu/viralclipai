#![deny(unreachable_patterns)]
//! Video processing worker.
//!
//! This crate provides:
//! - Job executor for process/reprocess jobs
//! - Clip rendering and upload
//! - Progress emission
//! - Graceful shutdown
//! - New modular architecture with security and performance

pub mod clip_pipeline;
pub mod config;
pub mod download_source_job;
pub mod error;
pub mod executor;
pub mod gemini;
pub mod logging;
pub mod neural_analysis_job;
pub mod neural_cache;
pub mod processor;
pub mod raw_segment_cache;
pub mod render_job;
pub mod reprocessing;
pub mod retry;
pub mod scene_analysis;
pub mod source_video_coordinator;

pub use config::WorkerConfig;
pub use error::{WorkerError, WorkerResult};
pub use executor::JobExecutor;
pub use logging::JobLogger;
pub use neural_cache::NeuralCacheService;
pub use processor::{EnhancedProcessingContext, VideoProcessor};
pub use raw_segment_cache::RawSegmentCacheService;
pub use scene_analysis::SceneAnalysisService;
pub use source_video_coordinator::SourceVideoCoordinator;
