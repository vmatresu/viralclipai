//! Cinematic analysis status tracking.
//!
//! This module provides types to track the status of cinematic/neural analysis
//! for the analysis-first processing pattern required by the Cinematic tier.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Timeout for cinematic analysis (24 hours in seconds).
pub const CINEMATIC_ANALYSIS_TIMEOUT_SECS: u64 = 86400;

/// Status of cinematic analysis for a scene.
///
/// The Cinematic tier requires analysis to complete before processing.
/// This enum tracks the analysis state in Redis for job coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CinematicAnalysisStatus {
    /// Analysis has not been started.
    NotStarted,

    /// Analysis is in progress.
    InProgress {
        /// When the analysis started
        started_at: DateTime<Utc>,
    },

    /// Analysis completed successfully.
    Complete {
        /// When the analysis completed
        completed_at: DateTime<Utc>,
    },

    /// Analysis failed.
    Failed {
        /// Error message describing the failure
        error: String,
        /// When the analysis failed
        failed_at: DateTime<Utc>,
    },
}

impl CinematicAnalysisStatus {
    /// Create a new in-progress status.
    pub fn in_progress() -> Self {
        Self::InProgress {
            started_at: Utc::now(),
        }
    }

    /// Create a new complete status.
    pub fn complete() -> Self {
        Self::Complete {
            completed_at: Utc::now(),
        }
    }

    /// Create a new failed status.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::Failed {
            error: error.into(),
            failed_at: Utc::now(),
        }
    }

    /// Check if analysis is complete.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    /// Check if analysis is in progress.
    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::InProgress { .. })
    }

    /// Check if analysis has failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Check if analysis has timed out.
    ///
    /// Returns true if the analysis has been in progress for longer than the timeout.
    pub fn is_timed_out(&self) -> bool {
        self.is_timed_out_with_secs(CINEMATIC_ANALYSIS_TIMEOUT_SECS)
    }

    /// Check if analysis has timed out with a custom timeout.
    pub fn is_timed_out_with_secs(&self, timeout_secs: u64) -> bool {
        if let Self::InProgress { started_at } = self {
            let elapsed = Utc::now().signed_duration_since(*started_at);
            elapsed.num_seconds() > timeout_secs as i64
        } else {
            false
        }
    }

    /// Get the error message if failed.
    pub fn error_message(&self) -> Option<&str> {
        if let Self::Failed { error, .. } = self {
            Some(error)
        } else {
            None
        }
    }
}

impl Default for CinematicAnalysisStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

/// Redis key pattern for cinematic analysis status.
///
/// Format: `cinematic:analysis:{video_id}:{scene_id}:status`
pub fn cinematic_analysis_key(video_id: &str, scene_id: u32) -> String {
    format!("cinematic:analysis:{}:{}:status", video_id, scene_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_creation() {
        let not_started = CinematicAnalysisStatus::NotStarted;
        assert!(!not_started.is_complete());
        assert!(!not_started.is_in_progress());
        assert!(!not_started.is_failed());

        let in_progress = CinematicAnalysisStatus::in_progress();
        assert!(in_progress.is_in_progress());
        assert!(!in_progress.is_complete());

        let complete = CinematicAnalysisStatus::complete();
        assert!(complete.is_complete());
        assert!(!complete.is_in_progress());

        let failed = CinematicAnalysisStatus::failed("some error");
        assert!(failed.is_failed());
        assert_eq!(failed.error_message(), Some("some error"));
    }

    #[test]
    fn test_timeout_check() {
        // Create a status that started 2 hours ago
        let two_hours_ago = Utc::now() - chrono::Duration::hours(2);
        let status = CinematicAnalysisStatus::InProgress { started_at: two_hours_ago };
        
        // Should not be timed out with default 24h timeout
        assert!(!status.is_timed_out());
        
        // Should be timed out with 1 hour timeout
        assert!(status.is_timed_out_with_secs(3600));
        
        // Fresh status should not be timed out
        let fresh = CinematicAnalysisStatus::in_progress();
        assert!(!fresh.is_timed_out());
    }

    #[test]
    fn test_redis_key_format() {
        let key = cinematic_analysis_key("video123", 5);
        assert_eq!(key, "cinematic:analysis:video123:5:status");
    }

    #[test]
    fn test_serialization() {
        let status = CinematicAnalysisStatus::in_progress();
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"in_progress\""));

        let parsed: CinematicAnalysisStatus = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_in_progress());
    }

    #[test]
    fn test_default() {
        let status = CinematicAnalysisStatus::default();
        assert!(matches!(status, CinematicAnalysisStatus::NotStarted));
    }
}
