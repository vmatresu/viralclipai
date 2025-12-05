//! Integration test runner.
//!
//! Run all integration tests:
//!   cargo test --test integration
//!
//! Run only tests that don't require external services:
//!   cargo test --test integration -- --skip ignored
//!
//! Run tests that require external services:
//!   cargo test --test integration -- --ignored

mod integration;

pub use integration::*;
