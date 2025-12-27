//! Premium intelligent_speaker style implementation.
//!
//! This module provides enhanced camera tracking for the `intelligent_speaker` style,
//! the highest tier full-format Active Speaker mode.
//!
//! # Features
//!
//! - **Smart Target Selection**: Selects primary subject with stability over time
//!   using PURELY VISUAL signals (NO audio)
//! - **Vertical Bias Framing**: Places eyes in upper third of frame
//! - **Zoom-aware Dead-zone**: Camera responsiveness adapts to zoom level
//! - **Smooth Transitions**: Exponential smoothing with pan/zoom speed limits
//! - **Scene Change Detection**: Fast adaptation on scene cuts
//! - **Dropout Resilience**: Holds position during brief detection gaps
//! - **Real Timestamps**: Uses actual detection timestamps for accurate dt
//!
//! # Visual-Only Scoring
//!
//! Subject selection uses these visual signals ONLY:
//! - Face size/prominence
//! - Detection confidence
//! - Mouth/facial activity (from face mesh, NOT audio)
//! - Track stability (age + jitter)
//! - Geometric centering
//!
//! # Module Structure
//!
//! - `config`: Configuration parameters for the premium style
//! - `target_selector`: Subject selection with visual activity tracking
//! - `smoothing`: Camera motion smoothing with zoom-aware dead-zone
//! - `crop_computer`: Aspect-ratio aware crop computation
//! - `camera_planner`: Orchestrates the full pipeline with real timestamps

pub mod camera_planner;
pub mod config;
pub mod crop_computer;
pub mod smoothing;
pub mod target_selector;

pub use camera_planner::{PlannerStats, PremiumCameraPlanner};
pub use config::PremiumSpeakerConfig;
pub use crop_computer::{CropComputeConfig, CropComputer};
pub use smoothing::{CameraState, PremiumSmoother};
pub use target_selector::{CameraTargetSelector, FocusPoint, VisualScores};
