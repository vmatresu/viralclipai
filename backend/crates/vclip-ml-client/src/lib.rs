//! Client for Python ML service (intelligent cropping).
//!
//! This crate provides a client to interact with the Python intelligent-crop
//! service during the transition period. The service exposes face detection
//! and crop planning functionality.
//!
//! In the future (Phase J), this will be replaced with native Rust ML.

pub mod client;
pub mod error;
pub mod types;

pub use client::MlClient;
pub use error::{MlError, MlResult};
pub use types::{CropPlan, CropRequest, CropWindow};
