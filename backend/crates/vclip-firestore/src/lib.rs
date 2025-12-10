//! Firestore REST API client.
//!
//! This crate provides:
//! - Typed repositories for Videos and Clips
//! - Service account authentication via gcp_auth
//! - Merge updates and retry logic

pub mod client;
pub mod error;
pub mod repos;
pub mod types;

pub use client::FirestoreClient;
pub use error::{FirestoreError, FirestoreResult};
pub use repos::{ClipRepository, ShareRepository, ShareSlugIndex, VideoRepository};
pub use types::{Document, FromFirestoreValue, ToFirestoreValue, Value};
