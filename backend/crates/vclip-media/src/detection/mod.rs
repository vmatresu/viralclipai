//! Detection module for tiered video analysis pipelines.
//!
//! This module provides composable detection pipelines that vary by `DetectionTier`:
//!
//! | Tier | Face Detection | Audio Activity | Face Activity |
//! |------|----------------|----------------|---------------|
//! | `None` | — | — | — |
//! | `Basic` | YuNet | — | — |
//! | `SpeakerAware` | YuNet | SpeakerDetector | FaceActivityAnalyzer |
//! | `MotionAware` | YuNet | — | — |
//! | `ActivityAware` | YuNet | — | FaceActivityAnalyzer |
//!
//! Use `PipelineBuilder` to create pipelines with automatic fallback handling.

pub mod pipeline;
pub mod pipeline_builder;
pub mod providers;

pub use pipeline::{DetectionPipeline, DetectionResult, FrameResult};
pub use pipeline_builder::PipelineBuilder;
pub use providers::{AudioProvider, FaceActivityProvider, FaceProvider};
