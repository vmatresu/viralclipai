//! Integration tests for the Rust backend.
//!
//! These tests require external services (Redis, Firestore, R2) to be available.
//! Run with: `cargo test --test integration -- --ignored`

pub mod redis_tests;
pub mod firestore_tests;
pub mod storage_tests;
pub mod api_tests;
