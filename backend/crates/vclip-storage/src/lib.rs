//! Cloudflare R2 storage client.
//!
//! This crate provides:
//! - File upload/download to R2
//! - Presigned URL generation
//! - Clip and highlight listing
//! - File deletion
//! - Secure video delivery (playback/download/share URLs)

pub mod client;
pub mod delivery;
pub mod error;
pub mod operations;

pub use client::R2Client;
pub use delivery::{DeliveryConfig, DeliveryScope, DeliveryToken, DeliveryUrl, DeliveryUrlGenerator};
pub use error::{StorageError, StorageResult};
pub use operations::{ClipInfo, HighlightsData};
