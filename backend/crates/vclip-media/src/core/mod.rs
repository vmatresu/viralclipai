//! Core domain types and interfaces for video processing architecture.
//!
//! This module defines the fundamental types, traits, and interfaces that
//! form the backbone of the video processing system following Domain-Driven Design.

use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Semaphore;

use vclip_models::{ClipTask, EncodingConfig, Style};

use crate::error::MediaResult;

// Re-export implementations
pub use security::SecurityContext;
pub use observability::MetricsCollector;
pub use progress::{ProgressEvent, ProgressSender, ProgressReceiver, channel as progress_channel, noop_sender};

pub mod security;
pub mod observability;
pub mod performance;
pub mod infrastructure;
pub mod progress;

/// Core domain entity representing a video processing request.
/// Wraps the task with additional context and validation.
#[derive(Debug, Clone)]
pub struct ProcessingRequest {
    pub task: ClipTask,
    pub input_path: Arc<Path>,
    pub output_path: Arc<Path>,
    pub encoding: EncodingConfig,
    pub request_id: String,
    pub user_id: String,
}

impl ProcessingRequest {
    /// Create a new processing request with validation.
    pub fn new(
        task: ClipTask,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        encoding: EncodingConfig,
        request_id: String,
        user_id: String,
    ) -> MediaResult<Self> {
        // Validate paths exist and are accessible
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        if !input_path.exists() {
            return Err(crate::error::MediaError::InvalidVideo(
                format!("Input video does not exist: {}", input_path.display())
            ));
        }

        // Validate output directory is writable
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                return Err(crate::error::MediaError::InvalidVideo(
                    format!("Output directory does not exist: {}", parent.display())
                ));
            }
        }

        Ok(Self {
            task,
            input_path: Arc::from(input_path),
            output_path: Arc::from(output_path),
            encoding,
            request_id,
            user_id,
        })
    }

    /// Get the style for this request.
    pub fn style(&self) -> Style {
        self.task.style
    }

    /// Check if this is an intelligent style requiring face detection.
    pub fn requires_intelligence(&self) -> bool {
        self.task.style.requires_intelligent_crop()
    }
}

/// Processing context containing shared resources and configuration.
/// Passed to all style processors for consistent resource access.
#[derive(Clone)]
pub struct ProcessingContext {
    pub request_id: String,
    pub user_id: String,
    pub temp_dir: Arc<Path>,
    pub semaphore: Arc<Semaphore>,
    pub metrics: Arc<MetricsCollector>,
    pub security: Arc<SecurityContext>,
}

impl ProcessingContext {
    /// Create a new processing context.
    pub fn new(
        request_id: String,
        user_id: String,
        temp_dir: impl AsRef<Path>,
        semaphore: Arc<Semaphore>,
        metrics: Arc<MetricsCollector>,
        security: Arc<SecurityContext>,
    ) -> Self {
        Self {
            request_id,
            user_id,
            temp_dir: Arc::from(temp_dir.as_ref()),
            semaphore,
            metrics,
            security,
        }
    }
}

/// Core trait defining the interface for all style processors.
/// Follows Interface Segregation Principle - each processor implements only what it needs.
#[async_trait]
pub trait StyleProcessor: Send + Sync {
    /// Get the name of this processor for logging and debugging.
    fn name(&self) -> &'static str;

    /// Check if this processor can handle the given style.
    fn can_handle(&self, style: Style) -> bool;

    /// Get the priority of this processor (higher = preferred).
    fn priority(&self) -> i32 { 0 }

    /// Validate that this processor can handle the given request.
    /// Called before processing to ensure compatibility.
    async fn validate(&self, request: &ProcessingRequest, ctx: &ProcessingContext) -> MediaResult<()> {
        // Default implementation does basic validation
        if !self.can_handle(request.style()) {
            return Err(crate::error::MediaError::InvalidVideo(
                format!("Style {} not supported by processor {}", request.style(), self.name())
            ));
        }

        // Validate resource availability
        if ctx.semaphore.available_permits() == 0 {
            return Err(crate::error::MediaError::ResourceLimit(
                "No FFmpeg permits available".to_string()
            ));
        }

        Ok(())
    }

    /// Process the video according to the style requirements.
    /// This is the main processing method that implementations must provide.
    async fn process(&self, request: ProcessingRequest, ctx: ProcessingContext) -> MediaResult<ProcessingResult>;

    /// Estimate processing time and resources needed.
    /// Used for scheduling and resource allocation.
    fn estimate_complexity(&self, _request: &ProcessingRequest) -> ProcessingComplexity {
        ProcessingComplexity::default()
    }

    /// Get supported input/output formats for this style.
    fn supported_formats(&self) -> &'static [&'static str] {
        &["mp4", "mov", "avi", "mkv"]
    }
}

/// Result of a video processing operation.
/// Contains metadata about the processing and any generated artifacts.
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub output_path: Arc<Path>,
    pub thumbnail_path: Option<Arc<Path>>,
    pub duration_seconds: f64,
    pub file_size_bytes: u64,
    pub processing_time_ms: u64,
    pub metadata: ProcessingMetadata,
}

/// Additional metadata about the processing operation.
#[derive(Debug, Clone, Default)]
pub struct ProcessingMetadata {
    pub frames_processed: Option<u32>,
    pub faces_detected: Option<u32>,
    pub crop_windows: Option<u32>,
    pub ffmpeg_commands: Vec<String>,
    pub warnings: Vec<String>,
}

/// Complexity estimation for processing operations.
/// Used for resource allocation and scheduling decisions.
#[derive(Debug, Clone)]
pub struct ProcessingComplexity {
    pub estimated_time_ms: u64,
    pub cpu_usage: f32, // 0.0 to 1.0
    pub memory_mb: u32,
    pub temp_space_mb: u32,
}

impl Default for ProcessingComplexity {
    fn default() -> Self {
        Self {
            estimated_time_ms: 30_000, // 30 seconds default
            cpu_usage: 0.5,
            memory_mb: 256,
            temp_space_mb: 512,
        }
    }
}

/// Factory trait for creating style processors.
/// Allows dependency injection and testing.
#[async_trait]
pub trait StyleProcessorFactory: Send + Sync {
    async fn create_processor(&self, style: Style) -> MediaResult<Box<dyn StyleProcessor>>;
}

/// Registry for managing style processors.
/// Central hub for processor discovery and instantiation.
pub struct StyleProcessorRegistry {
    factories: Vec<Arc<dyn StyleProcessorFactory>>,
}

impl StyleProcessorRegistry {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
        }
    }

    /// Register a factory for creating processors.
    pub fn register_factory(&mut self, factory: Arc<dyn StyleProcessorFactory>) {
        self.factories.push(factory);
    }

    /// Get a processor for a given style.
    pub async fn get_processor(&self, style: Style) -> MediaResult<Box<dyn StyleProcessor>> {
        // Create a new processor using factories
        for factory in &self.factories {
            if let Ok(processor) = factory.create_processor(style).await {
                return Ok(processor);
            }
        }

        Err(crate::error::MediaError::InvalidVideo(
            format!("No processor available for style {}", style)
        ))
    }
}

impl Default for StyleProcessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
