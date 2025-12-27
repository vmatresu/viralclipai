//! Security module for video processing.
//!
//! Provides input validation, path sanitization, command injection prevention,
//! and resource limits to ensure safe processing operations.
//!
//! # FFmpeg Filter Security
//!
//! FFmpeg filter strings (used with `-vf` and `-filter_complex`) require special handling
//! because they legitimately contain characters that would be dangerous in shell contexts:
//! - `;` - filter chain separator
//! - `[` and `]` - stream labels like `[full]`, `[left]`
//! - `:` - parameter separator
//!
//! These are safe because:
//! 1. FFmpeg arguments are passed directly via `Command::args()`, not through a shell
//! 2. Filter strings are constructed programmatically, not from user input
//! 3. The validation distinguishes between filter arguments and other arguments

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{MediaError, MediaResult};

/// Arguments that expect FFmpeg filter syntax as their value.
/// These arguments allow special characters that are valid in FFmpeg filter expressions.
const FFMPEG_FILTER_ARGS: &[&str] = &[
    "-vf",
    "-af",
    "-filter_complex",
    "-filter:v",
    "-filter:a",
    "-lavfi",
];

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
        for cmd in [
            "rm", "rmdir", "del", "delete", "format", "fdisk", "mkfs", "dd", "wget", "curl",
        ] {
            blocked_commands.insert(cmd.to_string());
        }

        Self {
            allowed_input_extensions,
            allowed_output_extensions,
            max_file_size_mb: 2048,            // 2GB limit
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
                "Path contains null bytes".to_string(),
            ));
        }

        // Check for path traversal attempts
        let path_str = path.to_string_lossy();
        for pattern in &self.path_traversal_patterns {
            if path_str.contains(pattern) {
                return Err(MediaError::SecurityViolation(format!(
                    "Path traversal attempt detected: {}",
                    pattern
                )));
            }
        }

        // Validate file extension for known safe types
        if let Some(extension) = path.extension() {
            let ext_str = extension.to_string_lossy().to_lowercase();

            // Check if it's an allowed input or output extension
            let is_allowed = self.allowed_input_extensions.contains(&ext_str)
                || self.allowed_output_extensions.contains(&ext_str);

            if !is_allowed {
                return Err(MediaError::SecurityViolation(format!(
                    "File extension not allowed: {}",
                    ext_str
                )));
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
                return Err(MediaError::SecurityViolation(format!(
                    "Path not within allowed directories: {}",
                    path_abs.display()
                )));
            }
        }

        Ok(())
    }

    /// Validate file size is within limits.
    pub fn validate_file_size(&self, size_bytes: u64) -> MediaResult<()> {
        let size_mb = size_bytes / (1024 * 1024);
        if size_mb > self.max_file_size_mb {
            return Err(MediaError::SecurityViolation(format!(
                "File size {}MB exceeds maximum {}MB",
                size_mb, self.max_file_size_mb
            )));
        }
        Ok(())
    }

    /// Sanitize and validate FFmpeg command arguments.
    ///
    /// This method is context-aware: it recognizes FFmpeg filter arguments
    /// (like `-vf`, `-filter_complex`) and allows filter syntax characters
    /// in their values, while still blocking dangerous characters in other arguments.
    ///
    /// # Security Model
    ///
    /// Since we use `Command::args()` to pass arguments directly to FFmpeg
    /// (not through a shell), shell metacharacters like `;` and `|` cannot
    /// cause command injection. However, we still validate:
    /// - No blocked commands in arguments
    /// - No shell metacharacters in non-filter arguments
    /// - Valid paths for file arguments
    ///
    /// FFmpeg filter arguments are allowed to contain:
    /// - `;` (filter chain separator)
    /// - `[` and `]` (stream labels)
    /// - `:` (parameter separator)
    /// - `(` and `)` (filter expressions)
    pub fn sanitize_command(&self, args: &[String]) -> MediaResult<Vec<String>> {
        let mut sanitized = Vec::with_capacity(args.len());
        let mut expect_filter_value = false;

        for arg in args {
            // Check for blocked commands
            if self.blocked_commands.contains(arg) {
                return Err(MediaError::SecurityViolation(format!(
                    "Blocked command in arguments: {}",
                    arg
                )));
            }

            // Check if this is a filter argument flag
            if FFMPEG_FILTER_ARGS.contains(&arg.as_str()) {
                expect_filter_value = true;
                sanitized.push(arg.clone());
                continue;
            }

            // If we're expecting a filter value, validate it with relaxed rules
            if expect_filter_value {
                self.validate_ffmpeg_filter_arg(arg)?;
                expect_filter_value = false;
                sanitized.push(arg.clone());
                continue;
            }

            // For non-filter arguments, check for dangerous shell metacharacters
            // Note: `:` is allowed as it's common in timestamps and codec options
            let dangerous_chars = [';', '&', '|', '`', '$', '<', '>', '\n', '\r'];
            if arg.chars().any(|c| dangerous_chars.contains(&c)) {
                return Err(MediaError::SecurityViolation(format!(
                    "Dangerous character in argument: {}",
                    arg
                )));
            }

            // Validate paths in arguments (skip flags)
            if !arg.starts_with('-') {
                // If it looks like a path, validate it
                if arg.contains('/') || arg.contains('\\') {
                    let path = Path::new(arg);
                    if path.exists() || path.is_absolute() {
                        self.validate_path(path)?;
                    }
                }
            }

            sanitized.push(arg.clone());
        }

        Ok(sanitized)
    }

    /// Validate an FFmpeg filter argument value.
    ///
    /// Filter arguments can contain characters that would be dangerous in shell
    /// contexts but are safe when passed directly to FFmpeg via `Command::args()`.
    ///
    /// Allowed: `;`, `[`, `]`, `:`, `(`, `)`, `=`, `,`, `/`, `-`, `_`, `.`, alphanumeric
    /// Blocked: `&`, `|`, `$`, `` ` ``, `<`, `>`, `\n`, `\r`, null bytes
    fn validate_ffmpeg_filter_arg(&self, filter: &str) -> MediaResult<()> {
        // Check for null bytes (always dangerous)
        if filter.contains('\0') {
            return Err(MediaError::SecurityViolation(
                "Filter contains null bytes".to_string(),
            ));
        }

        // These characters could indicate shell injection attempts even in filter context
        let dangerous_chars = ['&', '|', '`', '$', '<', '>', '\n', '\r'];
        if filter.chars().any(|c| dangerous_chars.contains(&c)) {
            return Err(MediaError::SecurityViolation(format!(
                "Dangerous character in filter: {}",
                filter
            )));
        }

        // Validate that the filter only contains expected FFmpeg filter syntax
        // This is a whitelist approach for extra safety
        for c in filter.chars() {
            let is_valid = c.is_alphanumeric()
                || c == ';'  // filter chain separator
                || c == '['  // stream label start
                || c == ']'  // stream label end
                || c == ':'  // parameter separator
                || c == '('  // expression start
                || c == ')'  // expression end
                || c == '='  // assignment
                || c == ','  // list separator
                || c == '/'  // path separator, division
                || c == '-'  // negative numbers, flags
                || c == '_'  // identifiers
                || c == '.'  // decimals
                || c == ' '  // spaces in some filters
                || c == '\'' // quoted strings
                || c == '"'  // quoted strings
                || c == '*'  // wildcards in some filters
                || c == '+'  // addition
                || c == '#'  // color codes
                || c == '@'  // some filter options
                || c == '%'  // modulo
                || c == '^'  // power
                || c == '!'  // negation in expressions
                || c == '?'  // ternary in expressions
                || c == '\\'  // escape sequences
                ;

            if !is_valid {
                return Err(MediaError::SecurityViolation(format!(
                    "Invalid character '{}' in filter",
                    c
                )));
            }
        }

        Ok(())
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
            _ => Ok(()),
        }
    }

    /// Validate processing time estimate is within limits.
    pub fn validate_processing_time(&self, estimated_seconds: u64) -> MediaResult<()> {
        if estimated_seconds > self.max_processing_time_seconds {
            return Err(MediaError::SecurityViolation(format!(
                "Estimated processing time {}s exceeds maximum {}s",
                estimated_seconds, self.max_processing_time_seconds
            )));
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
        security: &SecurityContext,
    ) -> MediaResult<()> {
        // Validate input path
        security.validate_path(input_path)?;

        // Validate output path
        security.validate_path(output_path)?;

        // Check file size if input exists
        if input_path.exists() {
            let metadata = input_path.metadata().map_err(|e| {
                MediaError::InvalidVideo(format!("Cannot read input file metadata: {}", e))
            })?;
            security.validate_file_size(metadata.len())?;
        }

        Ok(())
    }

    /// Validate an FFmpeg filter string.
    ///
    /// This function validates that a filter string is safe to use with FFmpeg.
    /// It allows all valid FFmpeg filter syntax characters (`;`, `[`, `]`, `:`, etc.)
    /// while blocking truly dangerous characters that could indicate injection attempts.
    ///
    /// # Note
    ///
    /// This function does NOT strip characters - it validates and returns the original
    /// filter unchanged if valid. FFmpeg filter syntax requires `;` for chain separation
    /// and `[`/`]` for stream labels.
    pub fn validate_ffmpeg_filter(filter: &str, security: &SecurityContext) -> MediaResult<String> {
        // Use the SecurityContext's filter validation
        security.validate_ffmpeg_filter_arg(filter)?;

        // Additional check for potentially dangerous filter operations
        // These filters can execute external commands or access files unexpectedly
        let dangerous_filters = ["sendcmd", "zmq", "movie", "amovie"];
        for dangerous in &dangerous_filters {
            // Check for the filter name followed by = or end of string/chain
            // This avoids false positives like "selectivecolor" matching "select"
            let patterns = [
                format!("{}=", dangerous),
                format!("{};", dangerous),
                format!("[{}]", dangerous),
            ];
            for pattern in &patterns {
                if filter.contains(pattern) {
                    return Err(MediaError::SecurityViolation(format!(
                        "Dangerous filter operation: {}",
                        dangerous
                    )));
                }
            }
            // Also check if filter ends with the dangerous name
            if filter.ends_with(dangerous) {
                return Err(MediaError::SecurityViolation(format!(
                    "Dangerous filter operation: {}",
                    dangerous
                )));
            }
        }

        // Return the original filter unchanged
        Ok(filter.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_command_allows_ffmpeg_filter_syntax() {
        let ctx = SecurityContext::new();

        // This is the exact filter that was causing the false positive
        let filter = "scale=1920:-2,split=2[full][full2];[full]crop=910:1080:0:0[left];[full2]crop=960:1080:960:0[right];[left]scale=1080:-2,crop=1080:960[left_scaled];[right]scale=1080:-2,crop=1080:960[right_scaled];[left_scaled][right_scaled]vstack=inputs=2";

        let args = vec![
            "-y".to_string(),
            "-i".to_string(),
            "input.mp4".to_string(),
            "-vf".to_string(),
            filter.to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "output.mp4".to_string(),
        ];

        let result = ctx.sanitize_command(&args);
        assert!(
            result.is_ok(),
            "Filter should be allowed: {:?}",
            result.err()
        );

        let sanitized = result.unwrap();
        assert_eq!(sanitized.len(), args.len());
        assert_eq!(sanitized[4], filter); // Filter should be unchanged
    }

    #[test]
    fn test_sanitize_command_allows_filter_complex() {
        let ctx = SecurityContext::new();

        let filter = "[0:v][1:v]vstack=inputs=2";

        let args = vec![
            "-i".to_string(),
            "left.mp4".to_string(),
            "-i".to_string(),
            "right.mp4".to_string(),
            "-filter_complex".to_string(),
            filter.to_string(),
            "output.mp4".to_string(),
        ];

        let result = ctx.sanitize_command(&args);
        assert!(
            result.is_ok(),
            "filter_complex should be allowed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_sanitize_command_blocks_dangerous_in_non_filter() {
        let ctx = SecurityContext::new();

        // Shell injection attempt in non-filter argument
        let args = vec![
            "-i".to_string(),
            "input.mp4; rm -rf /".to_string(), // Dangerous!
        ];

        let result = ctx.sanitize_command(&args);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Dangerous character"));
    }

    #[test]
    fn test_sanitize_command_blocks_dangerous_in_filter() {
        let ctx = SecurityContext::new();

        // Shell injection attempt in filter (backticks)
        let args = vec!["-vf".to_string(), "scale=`rm -rf /`:1080".to_string()];

        let result = ctx.sanitize_command(&args);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Dangerous character"));
    }

    #[test]
    fn test_sanitize_command_blocks_pipe_in_filter() {
        let ctx = SecurityContext::new();

        let args = vec![
            "-vf".to_string(),
            "scale=1920:-2 | cat /etc/passwd".to_string(),
        ];

        let result = ctx.sanitize_command(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_ffmpeg_filter_allows_valid_syntax() {
        let ctx = SecurityContext::new();

        let valid_filters = [
            "scale=1920:-2",
            "crop=1080:1920:0:0",
            "scale=1920:-2,split=2[a][b];[a]crop=960:1080[left];[b]crop=960:1080:960:0[right]",
            "[0:v][1:v]vstack=inputs=2",
            "fps=30,scale=1080:1920:force_original_aspect_ratio=decrease",
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2",
            "eq=brightness=0.1:contrast=1.2",
        ];

        for filter in valid_filters {
            let result = validation::validate_ffmpeg_filter(filter, &ctx);
            assert!(
                result.is_ok(),
                "Filter '{}' should be valid: {:?}",
                filter,
                result.err()
            );
            assert_eq!(result.unwrap(), filter, "Filter should be unchanged");
        }
    }

    #[test]
    fn test_validate_ffmpeg_filter_blocks_dangerous_filters() {
        let ctx = SecurityContext::new();

        let dangerous_filters = [
            "sendcmd=c='0 volume 0'",
            "zmq=bind_address=tcp://127.0.0.1:5555",
            "movie=/etc/passwd",
        ];

        for filter in dangerous_filters {
            let result = validation::validate_ffmpeg_filter(filter, &ctx);
            assert!(result.is_err(), "Filter '{}' should be blocked", filter);
        }
    }

    #[test]
    fn test_validate_ffmpeg_filter_arg_whitelist() {
        let ctx = SecurityContext::new();

        // All these characters should be allowed in filters
        let valid_chars = "abcABC123;[]():=,/-_.'\"+#@%^!?\\";
        let result = ctx.validate_ffmpeg_filter_arg(valid_chars);
        assert!(
            result.is_ok(),
            "Valid chars should pass: {:?}",
            result.err()
        );

        // These should be blocked
        let invalid_cases = [
            "test&cmd",
            "test|cmd",
            "test`cmd",
            "test$var",
            "test<file",
            "test>file",
        ];
        for case in invalid_cases {
            let result = ctx.validate_ffmpeg_filter_arg(case);
            assert!(result.is_err(), "Invalid case '{}' should be blocked", case);
        }
    }

    #[test]
    fn test_colons_allowed_in_non_filter_args() {
        let ctx = SecurityContext::new();

        // Colons are common in timestamps and codec options
        let args = vec![
            "-ss".to_string(),
            "00:01:30".to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
        ];

        let result = ctx.sanitize_command(&args);
        assert!(
            result.is_ok(),
            "Colons should be allowed: {:?}",
            result.err()
        );
    }
}
