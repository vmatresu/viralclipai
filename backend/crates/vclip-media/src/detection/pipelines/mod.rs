//! Detection pipeline implementations.
//!
//! This module contains tier-specific detection pipeline implementations.
//! Each pipeline is responsible for analyzing video frames and producing
//! detection results appropriate for its tier.

mod basic;
mod motion_aware;
mod none;
mod speaker_aware;

pub use basic::BasicPipeline;
pub use motion_aware::MotionAwarePipeline;
pub use none::NonePipeline;
pub use speaker_aware::SpeakerAwarePipeline;
