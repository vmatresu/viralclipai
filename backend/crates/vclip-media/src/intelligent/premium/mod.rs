//! Premium intelligent_speaker style implementation.
//!
//! This module provides enhanced camera tracking for the `intelligent_speaker` style,
//! the highest tier full-format Active Speaker mode.
//!
//! # Features
//!
//! - **Smart Target Selection**: Selects primary subject with stability over time
//! - **Vertical Bias Framing**: Places eyes in upper third of frame
//! - **Dead-zone Hysteresis**: Camera locks until subject moves significantly
//! - **Smooth Transitions**: Exponential smoothing with max pan speed limits
//! - **Scene Change Detection**: Fast adaptation on scene cuts
//!
//! # Module Structure
//!
//! - `config`: Configuration parameters for the premium style
//! - `target_selector`: Subject selection with activity tracking
//! - `smoothing`: Camera motion smoothing algorithms
//! - `crop_computer`: Aspect-ratio aware crop computation
//! - `camera_planner`: Orchestrates the full pipeline

pub mod config;
pub mod target_selector;
pub mod smoothing;
pub mod crop_computer;
pub mod camera_planner;

pub use config::PremiumSpeakerConfig;
pub use target_selector::CameraTargetSelector;
pub use smoothing::PremiumSmoother;
pub use crop_computer::{CropComputer, CropComputeConfig};
pub use camera_planner::PremiumCameraPlanner;
