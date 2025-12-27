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
pub mod ipv6_rotation;
pub mod probe;
pub mod progress;
pub mod silence_removal;
pub mod styles;
pub mod thumbnail;
pub mod watermark;

// Core architecture exports
pub use core::{
    infrastructure::{
        circuit_breaker::{CircuitBreaker, CircuitState},
        metrics::ProductionMetricsCollector,
    },
    observability::MetricsCollector,
    performance::{FFmpegPool, ResourceManager},
    security::SecurityContext,
    ProcessingContext, ProcessingRequest, ProcessingResult, StyleProcessor,
    StyleProcessorFactory as StyleProcessorFactoryTrait, StyleProcessorRegistry,
};

// Style processor exports
pub use styles::StyleProcessorFactory;

// Existing exports for backward compatibility
pub use clip::{create_clip, extract_segment};
pub use command::{create_ffmpeg_command, FfmpegCommand, FfmpegRunner};
pub use download::{
    download_segment, download_video, get_writable_cookies_path, is_supported_url,
    likely_supports_segment_download, SegmentDownloadNotSupported,
};
pub use error::{MediaError, MediaResult};
pub use intelligent::create_intelligent_clip;
// Note: create_intelligent_split_clip is deprecated - use create_tier_aware_split_clip_with_cache instead
#[deprecated(
    since = "0.1.0",
    note = "Use create_tier_aware_split_clip_with_cache from intelligent module instead"
)]
pub use intelligent::create_intelligent_split_clip;
pub use probe::{probe_video, VideoInfo};
pub use progress::{FfmpegProgress, ProgressCallback};
pub use thumbnail::generate_thumbnail;
pub use watermark::{
    apply_watermark, apply_watermark_if_available, WatermarkConfig, DEFAULT_WATERMARK_PATH,
};

// IPv6 rotation for YouTube rate limit avoidance
pub use ipv6_rotation::{
    get_ipv6_pool_stats, get_random_ipv6_address, refresh_ipv6_pool, IPv6PoolStats,
};
