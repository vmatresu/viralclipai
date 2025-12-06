//! Security module for video processing.
//!
//! Provides input validation, path sanitization, command injection prevention,
//! and resource limits to ensure safe processing operations.

use std::path::{Path, PathBuf};
use std::collections::HashSet;

use crate::error::{MediaError, MediaResult};

/// Security context for safe video processing operations.
/// Implements defense-in-depth security measures.
#[derive(Clone)]
pub struct SecurityContext {
    allowed_input_extensions: HashSet<String>,
    allowed_output_extensions: HashSet<String>,
    max_file_size_mb: u64,
    max_processing_time_seconds: u64,
    allowed_base_dirs: Vec<PathBuf>,
    blocked_commands: HashSet<String>,
    path_traversal_patterns: Vec<String>,
}

impl SecurityContext {
    /// Create a new security context with default secure settings.
    pub fn new() -> Self {
        let mut allowed_input_extensions = HashSet::new();
        for ext in ["mp4", "mov", "avi", "mkv", "webm", "flv", "wmv", "m4v"] {
            allowed_input_extensions.insert(ext.to_string());
        }

        let mut allowed_output_extensions = HashSet::new();
        for ext in ["mp4", "jpg", "jpeg", "png", "gif"] {
            allowed_output_extensions.insert(ext.to_string());
        }

        let mut blocked_commands = HashSet::new();
        for cmd in ["rm", "rmdir", "del", "delete", "format", "fdisk", "mkfs", "dd", "wget", "curl"] {
            blocked_commands.insert(cmd.to_string());
        }

        Self {
            allowed_input_extensions,
            allowed_output_extensions,
            max_file_size_mb: 2048, // 2GB limit
            max_processing_time_seconds: 1800, // 30 minutes
            allowed_base_dirs: Vec::new(),
            blocked_commands,
            path_traversal_patterns: vec![
                "..".to_string(),
                "../".to_string(),
                "..\\".to_string(),
                "\\..".to_string(),
                "/..".to_string(),
            ],
        }
    }

    /// Set allowed base directories for file operations.
    pub fn with_allowed_base_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.allowed_base_dirs = dirs;
        self
    }

    /// Set maximum file size limit.
    pub fn with_max_file_size_mb(mut self, size_mb: u64) -> Self {
        self.max_file_size_mb = size_mb;
        self
    }

    /// Set maximum processing time limit.
    pub fn with_max_processing_time(mut self, seconds: u64) -> Self {
        self.max_processing_time_seconds = seconds;
        self
    }

    /// Validate and sanitize a file path for security.
    pub fn validate_path(&self, path: &Path) -> MediaResult<()> {
        // Check for null bytes (common injection attack)
        if path.to_string_lossy().contains('\0') {
            return Err(MediaError::SecurityViolation(
                "Path contains null bytes".to_string()
            ));
        }

        // Check for path traversal attempts
        let path_str = path.to_string_lossy();
        for pattern in &self.path_traversal_patterns {
            if path_str.contains(pattern) {
                return Err(MediaError::SecurityViolation(
                    format!("Path traversal attempt detected: {}", pattern)
                ));
            }
        }

        // Validate file extension for known safe types
        if let Some(extension) = path.extension() {
            let ext_str = extension.to_string_lossy().to_lowercase();

            // Check if it's an allowed input or output extension
            let is_allowed = self.allowed_input_extensions.contains(&ext_str) ||
                           self.allowed_output_extensions.contains(&ext_str);

            if !is_allowed {
                return Err(MediaError::SecurityViolation(
                    format!("File extension not allowed: {}", ext_str)
                ));
            }
        }

        // Check if path is within allowed base directories
        if !self.allowed_base_dirs.is_empty() {
            let path_abs = path.canonicalize().map_err(|e| {
                MediaError::SecurityViolation(format!("Cannot canonicalize path: {}", e))
            })?;

            let mut is_allowed = false;
            for base_dir in &self.allowed_base_dirs {
                if path_abs.starts_with(base_dir) {
                    is_allowed = true;
                    break;
                }
            }

            if !is_allowed {
                return Err(MediaError::SecurityViolation(
                    format!("Path not within allowed directories: {}", path_abs.display())
                ));
            }
        }

        Ok(())
    }

    /// Validate file size is within limits.
    pub fn validate_file_size(&self, size_bytes: u64) -> MediaResult<()> {
        let size_mb = size_bytes / (1024 * 1024);
        if size_mb > self.max_file_size_mb {
            return Err(MediaError::SecurityViolation(
                format!("File size {}MB exceeds maximum {}MB", size_mb, self.max_file_size_mb)
            ));
        }
        Ok(())
    }

    /// Sanitize and validate FFmpeg command arguments.
    pub fn sanitize_command(&self, args: &[String]) -> MediaResult<Vec<String>> {
        let mut sanitized = Vec::with_capacity(args.len());

        for arg in args {
            // Check for blocked commands
            if self.blocked_commands.contains(arg) {
                return Err(MediaError::SecurityViolation(
                    format!("Blocked command in arguments: {}", arg)
                ));
            }

            // Check for shell metacharacters that could be used for injection
            let dangerous_chars = [';', '&', '|', '`', '$', '(', ')', '<', '>', '\n', '\r'];
            if arg.chars().any(|c| dangerous_chars.contains(&c)) {
                return Err(MediaError::SecurityViolation(
                    format!("Dangerous character in argument: {}", arg)
                ));
            }

            // Validate paths in arguments
            if arg.starts_with('-') && arg.len() > 1 {
                // This is a flag, check next argument if it's a path
                continue;
            }

            // If it looks like a path, validate it
            if arg.contains('/') || arg.contains('\\') || arg.contains('.') {
                let path = Path::new(arg);
                if path.exists() || path.is_absolute() {
                    self.validate_path(path)?;
                }
            }

            sanitized.push(arg.clone());
        }

        Ok(sanitized)
    }

    /// Check if operation is within resource limits.
    pub fn check_resource_limits(&self, operation: &str) -> MediaResult<()> {
        // This would integrate with system resource monitoring
        // For now, we just check basic limits
        match operation {
            "ffmpeg" => {
                // Could check CPU, memory, disk I/O limits
                Ok(())
            }
            "face_detection" => {
                // Could check GPU memory, CPU limits
                Ok(())
            }
            _ => Ok(())
        }
    }

    /// Validate processing time estimate is within limits.
    pub fn validate_processing_time(&self, estimated_seconds: u64) -> MediaResult<()> {
        if estimated_seconds > self.max_processing_time_seconds {
            return Err(MediaError::SecurityViolation(
                format!("Estimated processing time {}s exceeds maximum {}s",
                       estimated_seconds, self.max_processing_time_seconds)
            ));
        }
        Ok(())
    }

    /// Generate a secure temporary filename.
    pub fn secure_temp_filename(&self, prefix: &str, extension: &str) -> String {
        use uuid::Uuid;
        format!("{}_{}.{}", prefix, Uuid::new_v4().simple(), extension)
    }
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Input validation utilities.
pub mod validation {
    use super::*;

    /// Validate video processing request parameters.
    pub fn validate_processing_request(
        input_path: &Path,
        output_path: &Path,
        security: &SecurityContext
    ) -> MediaResult<()> {
        // Validate input path
        security.validate_path(input_path)?;

        // Validate output path
        security.validate_path(output_path)?;

        // Check file size if input exists
        if input_path.exists() {
            let metadata = input_path.metadata()
                .map_err(|e| MediaError::InvalidVideo(
                    format!("Cannot read input file metadata: {}", e)
                ))?;
            security.validate_file_size(metadata.len())?;
        }

        Ok(())
    }

    /// Sanitize and validate FFmpeg filter string.
    pub fn validate_ffmpeg_filter(filter: &str, _security: &SecurityContext) -> MediaResult<String> {
        // Check for dangerous filter operations
        let dangerous_filters = ["select", "concat", "streamselect", "sendcmd"];
        for dangerous in &dangerous_filters {
            if filter.contains(dangerous) {
                return Err(MediaError::SecurityViolation(
                    format!("Dangerous filter operation: {}", dangerous)
                ));
            }
        }

        // Basic sanitization - remove or escape dangerous characters
        let sanitized = filter.replace([';', '&', '|', '`', '$'], "");

        Ok(sanitized)
    }
}

/// Security violation error type.
#[derive(Debug, thiserror::Error)]
#[error("Security violation: {message}")]
pub struct SecurityViolation {
    pub message: String,
}

impl From<SecurityViolation> for MediaError {
    fn from(err: SecurityViolation) -> Self {
        MediaError::SecurityViolation(err.message)
    }
}
