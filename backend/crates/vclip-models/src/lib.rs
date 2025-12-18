//! Shared data models for ViralClip backend.
//!
//! This crate provides Serde-serializable types for:
//! - Jobs and clip tasks
//! - Video styles and crop modes
//! - Encoding configuration
//! - Detection tiers for intelligent processing
//! - Redis pub/sub progress message schemas (ws.rs, used for worker progress)
//! - Plan configuration and storage limits
//! - Share link configuration
//! - Analysis workflow (drafts and scenes)
//! - Cinematic analysis status tracking

pub mod analysis;
pub mod cinematic_analysis;
pub mod clip;
pub mod credit_transaction;
pub mod detection_tier;
pub mod encoding;
pub mod highlight;
pub mod job;
pub mod job_status;
pub mod neural_analysis;
pub mod plan;
pub mod share;
pub mod style;
pub mod utils;
pub mod video;
pub mod ws;
pub mod youtube_url_config;

// Re-export common types
pub use clip::{
    ClipMetadata, ClipStatus, ClipTask, HorizontalPosition, StreamerParams, StreamerSplitParams,
    TopSceneEntry, VerticalPosition, sanitize_filename_title,
};
pub use detection_tier::DetectionTier;
pub use encoding::EncodingConfig;
pub use highlight::{Highlight, HighlightCategory, HighlightsData, VideoHighlights};
pub use job::{Job, JobId, JobState, JobType};
pub use plan::{format_bytes, PlanLimits, PlanTier, StorageAccounting, StorageUsage};
pub use plan::{FREE_STORAGE_LIMIT_BYTES, PRO_STORAGE_LIMIT_BYTES, STUDIO_STORAGE_LIMIT_BYTES};
pub use plan::{
    FREE_MONTHLY_CREDITS, PRO_MONTHLY_CREDITS, STUDIO_MONTHLY_CREDITS, MAX_CLIP_LENGTH_SECONDS,
    ANALYSIS_CREDIT_COST, SCENE_ORIGINALS_DOWNLOAD_COST, STREAMER_STYLE_COST,
    STREAMER_SPLIT_STYLE_COST, SILENT_REMOVER_ADDON_COST, OBJECT_DETECTION_ADDON_COST,
    credits_for_detection_tier,
};
pub use share::{CreateShareRequest, ShareAccessLevel, ShareConfig, ShareResponse, is_valid_share_slug, MAX_SHARE_EXPIRY_HOURS};
pub use style::{AspectRatio, CropMode, Style};
pub use utils::{extract_youtube_id, extract_youtube_id_legacy, YoutubeIdError, YoutubeIdResult};
pub use video::{ProcessingProgress, SourceVideoStatus, VideoId, VideoMetadata, VideoStatus};
pub use ws::{ClipProcessingStep, WsMessage, WsMessageType};
pub use youtube_url_config::{
    analyze_youtube_url, analyze_youtube_url_json, LiveCaptureMode, LiveHandling, SubtitlePlan,
    UrlType, ValidationResult, VideoDownloadPlan, YoutubeUrlConfig, YoutubeUrlInput,
};
pub use analysis::{
    AnalysisDraft, AnalysisStatus, AnalysisStatusResponse, DraftScene, ProcessDraftRequest,
    ProcessingEstimate, SceneSelection, StartAnalysisResponse,
};
pub use neural_analysis::{
    BoundingBox, CachedObjectDetection, CinematicSignalsCache, CropperDetection, FaceDetection,
    FrameAnalysis, FrameObjectDetections, ObjectDetectionsCache, SceneNeuralAnalysis,
    ShotBoundaryCache, CINEMATIC_SIGNALS_VERSION, NEURAL_ANALYSIS_VERSION,
};
pub use cinematic_analysis::{
    CinematicAnalysisStatus, cinematic_analysis_key, CINEMATIC_ANALYSIS_TIMEOUT_SECS,
};
pub use job_status::{JobStatus, JobStatusCache};
pub use credit_transaction::{CreditContext, CreditOperationType, CreditTransaction};
pub mod rect;

pub use rect::NormalizedRect;
