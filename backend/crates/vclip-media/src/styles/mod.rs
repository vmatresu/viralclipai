//! Style processors for video processing.
//!
//! Each video style (Original, Split, LeftFocus, etc.) has its own processor
//! implementing the StyleProcessor trait. This ensures complete separation
//! and testability of style-specific logic.

use std::path::Path;

use async_trait::async_trait;
use vclip_models::Style;
use crate::error::MediaResult;
use crate::core::{StyleProcessor, StyleProcessorFactory as StyleProcessorFactoryTrait};

pub mod original;
pub mod split;
pub mod left_focus;
pub mod right_focus;
pub mod intelligent;
pub mod intelligent_split;

/// Factory for creating style processors.
/// Implements dependency injection for testing and flexibility.
#[derive(Clone)]
pub struct StyleProcessorFactory {
    // Configuration can be added here for different environments
}

impl StyleProcessorFactory {
    /// Create a new factory.
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl StyleProcessorFactoryTrait for StyleProcessorFactory {
    /// Create a processor for the given style.
    async fn create_processor(&self, style: Style) -> MediaResult<Box<dyn StyleProcessor>> {
        match style {
            Style::Original => Ok(Box::new(original::OriginalProcessor::new())),
            Style::Split => Ok(Box::new(split::SplitProcessor::new())),
            Style::LeftFocus => Ok(Box::new(left_focus::LeftFocusProcessor::new())),
            Style::RightFocus => Ok(Box::new(right_focus::RightFocusProcessor::new())),
            Style::Intelligent => Ok(Box::new(intelligent::IntelligentProcessor::new())),
            Style::IntelligentSplit => Ok(Box::new(intelligent_split::IntelligentSplitProcessor::new())),
        }
    }
}

impl Default for StyleProcessorFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions shared across style processors.
pub mod utils {
    use super::*;

    /// Validate that input and output paths are accessible.
    pub fn validate_paths(input: &Path, output: &Path) -> MediaResult<()> {
        if !input.exists() {
            return Err(crate::error::MediaError::InvalidVideo(
                format!("Input file does not exist: {}", input.display())
            ));
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(crate::error::MediaError::InvalidVideo(
                    format!("Output directory does not exist: {}", parent.display())
                ));
            }
        }

        Ok(())
    }

    /// Generate thumbnail path from output path.
    pub fn thumbnail_path(output: &Path) -> std::path::PathBuf {
        output.with_extension("jpg")
    }

    /// Calculate processing complexity based on video properties.
    pub fn estimate_complexity(
        duration_seconds: f64,
        requires_intelligence: bool
    ) -> crate::core::ProcessingComplexity {
        let base_time = if requires_intelligence { 60_000 } else { 30_000 }; // ms
        let duration_factor = (duration_seconds / 60.0).max(1.0); // Scale by duration

        crate::core::ProcessingComplexity {
            estimated_time_ms: (base_time as f64 * duration_factor) as u64,
            cpu_usage: if requires_intelligence { 0.8 } else { 0.3 },
            memory_mb: if requires_intelligence { 512 } else { 128 },
            temp_space_mb: 256,
        }
    }
}
