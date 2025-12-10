//! Shared data models for ViralClip backend.
//!
//! This crate provides Serde-serializable types for:
//! - Jobs and clip tasks
//! - Video styles and crop modes
//! - Encoding configuration
//! - Detection tiers for intelligent processing
//! - WebSocket message schemas
//! - Plan configuration and storage limits

pub mod clip;
pub mod detection_tier;
pub mod encoding;
pub mod highlight;
pub mod job;
pub mod plan;
pub mod style;
pub mod utils;
pub mod video;
pub mod ws;

// Re-export common types
pub use clip::{ClipMetadata, ClipStatus, ClipTask};
pub use detection_tier::DetectionTier;
pub use encoding::EncodingConfig;
pub use highlight::{Highlight, HighlightCategory};
pub use job::{Job, JobId, JobState, JobType};
pub use plan::{format_bytes, PlanLimits, PlanTier, StorageUsage};
pub use plan::{FREE_STORAGE_LIMIT_BYTES, PRO_STORAGE_LIMIT_BYTES, STUDIO_STORAGE_LIMIT_BYTES};
pub use style::{AspectRatio, CropMode, Style};
pub use utils::{extract_youtube_id, extract_youtube_id_legacy, YoutubeIdError, YoutubeIdResult};
pub use video::{VideoId, VideoMetadata, VideoStatus};
pub use ws::{ClipProcessingStep, WsMessage, WsMessageType};

