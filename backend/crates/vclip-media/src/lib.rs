#![deny(unreachable_patterns)]
//! FFmpeg CLI wrapper for video processing.
//!
//! This crate provides:
//! - Type-safe FFmpeg command building
//! - Progress parsing from `-progress pipe:2`
//! - Cancellation support via tokio
//! - All video operations (clip, segment, stack, thumbnail)
//! - Intelligent cropping with face detection and tracking
//! - Modular style processing architecture with security, performance, and observability

pub mod clip;
pub mod command;
pub mod core;
pub mod detection;
pub mod download;
pub mod error;
pub mod filters;
pub mod fs_utils;
pub mod intelligent;
pub mod probe;
pub mod progress;
pub mod styles;
pub mod thumbnail;

// Core architecture exports
pub use core::{
    ProcessingRequest, ProcessingResult, ProcessingContext, StyleProcessor, StyleProcessorRegistry,
    StyleProcessorFactory as StyleProcessorFactoryTrait,
    security::SecurityContext,
    observability::MetricsCollector,
    performance::{ResourceManager, FFmpegPool},
    infrastructure::{circuit_breaker::{CircuitBreaker, CircuitState}, metrics::ProductionMetricsCollector},
};

// Style processor exports
pub use styles::StyleProcessorFactory;

// Existing exports for backward compatibility
pub use clip::{create_clip, extract_segment};
pub use command::{FfmpegCommand, FfmpegRunner};
pub use download::{download_video, download_segment, is_supported_url, likely_supports_segment_download, SegmentDownloadNotSupported};
pub use error::{MediaError, MediaResult};
pub use intelligent::create_intelligent_clip;
// Note: create_intelligent_split_clip is deprecated - use create_tier_aware_split_clip_with_cache instead
#[deprecated(since = "0.1.0", note = "Use create_tier_aware_split_clip_with_cache from intelligent module instead")]
pub use intelligent::create_intelligent_split_clip;
pub use probe::{probe_video, VideoInfo};
pub use progress::{FfmpegProgress, ProgressCallback};
pub use thumbnail::generate_thumbnail;
