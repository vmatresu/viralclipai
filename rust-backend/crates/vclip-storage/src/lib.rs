//! Cloudflare R2 storage client.
//!
//! This crate provides:
//! - File upload/download to R2
//! - Presigned URL generation
//! - Clip and highlight listing
//! - File deletion

pub mod client;
pub mod error;
pub mod operations;

pub use client::R2Client;
pub use error::{StorageError, StorageResult};
pub use operations::{ClipInfo, HighlightsData};
