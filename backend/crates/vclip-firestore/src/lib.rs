//! Firestore REST API client.
//!
//! This crate provides:
//! - Typed repositories for Videos and Clips
//! - Share repository for clip share links
//! - Analysis draft repository for video analysis workflow
//! - Storage accounting repository for quota tracking (Phase 5)
//! - Service account authentication via gcp_auth
//! - Merge updates and retry logic

pub mod analysis_draft_repo;
pub mod client;
pub mod error;
pub mod highlights_repo;
pub mod repos;
pub mod share_repo;
pub mod storage_accounting;
pub mod types;

pub use analysis_draft_repo::AnalysisDraftRepository;
pub use client::FirestoreClient;
pub use error::{FirestoreError, FirestoreResult};
pub use highlights_repo::HighlightsRepository;
pub use repos::{ClipRepository, VideoRepository};
pub use share_repo::{ShareRepository, ShareSlugIndex};
pub use storage_accounting::StorageAccountingRepository;
pub use types::{Document, FromFirestoreValue, ToFirestoreValue, Value};
