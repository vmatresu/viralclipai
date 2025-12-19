//! Firestore REST API client.
//!
//! Production-grade client with:
//! - Token caching with refresh margin
//! - HTTP client tuning (pooling, timeouts)
//! - Exponential backoff with jitter
//! - Observability (tracing spans, metrics)
//!
//! ## Modules
//! - `client` - Main Firestore REST API client
//! - `token_cache` - Thread-safe access token caching
//! - `retry` - Retry policy with exponential backoff
//! - `metrics` - Prometheus metrics collection
//! - `repos` - Typed repositories for Videos and Clips
//! - `types` - Firestore document types and value conversions

pub mod analysis_draft_repo;
pub mod client;
#[cfg(test)]
mod client_tests;
pub mod credit_transaction_repo;
pub mod error;
pub mod highlights_repo;
pub mod metrics;
pub mod repos;
pub mod retry;
pub mod share_repo;
pub mod sorting;
pub mod storage_accounting;
pub mod token_cache;
pub mod types;
pub mod user_credits;

pub use analysis_draft_repo::AnalysisDraftRepository;
pub use client::{FirestoreClient, FirestoreConfig};
pub use credit_transaction_repo::CreditTransactionRepository;
pub use error::{FirestoreError, FirestoreResult};
pub use highlights_repo::HighlightsRepository;
pub use repos::{ClipRepository, VideoRepository};
pub use retry::RetryConfig;
pub use share_repo::{ShareRepository, ShareSlugIndex};
pub use storage_accounting::StorageAccountingRepository;
pub use types::{Document, FromFirestoreValue, ToFirestoreValue, Value};
pub use user_credits::{current_month_key, CreditChargeResult, UserCreditsRepository};

