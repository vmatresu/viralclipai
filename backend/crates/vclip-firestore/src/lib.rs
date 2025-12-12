//! Firestore REST API client.
//!
//! This crate provides:
//! - Typed repositories for Videos and Clips
//! - Analysis draft repository for video analysis workflow
//! - Storage accounting repository for quota tracking (Phase 5)
//! - Service account authentication via gcp_auth
//! - Merge updates and retry logic

pub mod analysis_draft_repo;
pub mod client;
pub mod error;
pub mod repos;
pub mod highlights_repo;
pub mod storage_accounting;
pub mod types;

pub use analysis_draft_repo::AnalysisDraftRepository;
pub use client::FirestoreClient;
pub use error::{FirestoreError, FirestoreResult};
pub use repos::{ClipRepository, ShareRepository, ShareSlugIndex, VideoRepository};
pub use highlights_repo::HighlightsRepository;
pub use storage_accounting::StorageAccountingRepository;
pub use types::{Document, FromFirestoreValue, ToFirestoreValue, Value};
