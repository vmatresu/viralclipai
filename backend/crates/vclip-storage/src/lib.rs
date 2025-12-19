//! Cloudflare R2 storage client.
//!
//! This crate provides:
//! - File upload/download to R2
//! - Presigned URL generation
//! - Clip and highlight listing
//! - File deletion
//! - Secure video delivery (playback/download/share URLs)
//! - Neural analysis cache (gzip-compressed JSON)

pub mod client;
pub mod delivery;
pub mod error;
pub mod neural_cache;
pub mod operations;
pub mod transcript_cache;

pub use client::R2Client;
pub use delivery::{DeliveryConfig, DeliveryScope, DeliveryToken, DeliveryUrl, DeliveryUrlGenerator};
pub use error::{StorageError, StorageResult};
pub use neural_cache::{
    compress_neural_analysis, decompress_neural_analysis, delete_neural_analysis,
    load_neural_analysis, neural_analysis_exists, neural_cache_key, store_neural_analysis,
    StoreResult as NeuralCacheStoreResult,
};
pub use transcript_cache::{
    compress_transcript, decompress_transcript, delete_transcript, load_transcript,
    store_transcript, transcript_cache_id_from_url, transcript_cache_key, transcript_exists,
    StoreResult as TranscriptCacheStoreResult,
};
pub use operations::HighlightsData;
