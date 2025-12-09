//! Structured job logging utilities.
//!
//! Provides consistent, structured logging for job processing with
//! tracing spans and contextual information.

use tracing::{info, warn, error, Span};
use vclip_models::JobId;

/// Job logger for structured logging with consistent formatting.
///
/// Provides a simple interface for logging job lifecycle events
/// with automatic contextual information (job ID, operation type).
#[derive(Debug, Clone)]
pub struct JobLogger {
    job_id: String,
    operation: String,
}

impl JobLogger {
    /// Create a new job logger for a specific job and operation.
    ///
    /// # Arguments
    /// * `job_id` - The unique identifier for the job
    /// * `operation` - The type of operation (e.g., "video_processing", "render_scene_style")
    pub fn new(job_id: &JobId, operation: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            operation: operation.to_string(),
        }
    }

    /// Create a new job logger from a string job ID.
    pub fn from_string(job_id: &str, operation: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            operation: operation.to_string(),
        }
    }

    /// Log the start of a job operation.
    pub fn log_start(&self, message: &str) {
        info!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job started: {}", message
        );
    }

    /// Log a progress update during job execution.
    pub fn log_progress(&self, message: &str) {
        info!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job progress: {}", message
        );
    }

    /// Log a warning during job execution.
    pub fn log_warning(&self, message: &str) {
        warn!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job warning: {}", message
        );
    }

    /// Log an error during job execution.
    pub fn log_error(&self, message: &str) {
        error!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job error: {}", message
        );
    }

    /// Log the completion of a job operation.
    pub fn log_completion(&self, message: &str) {
        info!(
            job_id = %self.job_id,
            operation = %self.operation,
            "Job completed: {}", message
        );
    }

    /// Get the job ID.
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    /// Get the operation type.
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Create a tracing span for this job.
    ///
    /// Use this for more complex scenarios where you need to attach
    /// additional structured data to traces.
    pub fn create_span(&self) -> Span {
        tracing::info_span!(
            "job",
            job_id = %self.job_id,
            operation = %self.operation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_logger_creation() {
        let job_id = JobId::new();
        let logger = JobLogger::new(&job_id, "test_operation");
        
        assert_eq!(logger.job_id(), job_id.to_string());
        assert_eq!(logger.operation(), "test_operation");
    }

    #[test]
    fn test_job_logger_from_string() {
        let logger = JobLogger::from_string("test-job-123", "render");
        
        assert_eq!(logger.job_id(), "test-job-123");
        assert_eq!(logger.operation(), "render");
    }
}
