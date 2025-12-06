//! FFmpeg CLI wrapper for video processing.
//!
//! This crate provides:
//! - Type-safe FFmpeg command building
//! - Progress parsing from `-progress pipe:2`
//! - Cancellation support via tokio
//! - All video operations (clip, segment, stack, thumbnail)
//! - Intelligent cropping with face detection and tracking

pub mod clip;
pub mod command;
pub mod download;
pub mod error;
pub mod filters;
pub mod intelligent;
pub mod probe;
pub mod progress;
pub mod thumbnail;

pub use clip::{create_clip, extract_segment};
pub use command::{FfmpegCommand, FfmpegRunner};
pub use download::{download_video, extract_youtube_id, is_supported_url};
pub use error::{MediaError, MediaResult};
pub use intelligent::{create_intelligent_clip, create_intelligent_split_clip};
pub use probe::{probe_video, VideoInfo};
pub use progress::{FfmpegProgress, ProgressCallback};
pub use thumbnail::generate_thumbnail;
