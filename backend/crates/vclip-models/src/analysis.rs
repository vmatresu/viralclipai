//! Analysis workflow data models.
//!
//! This module provides types for the two-step video analysis workflow:
//! 1. Analyze: Download transcript, detect scenes, create draft
//! 2. Process: User selects scenes and styles, enqueue render jobs

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Status of an analysis job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    /// Job is queued, waiting to start
    #[default]
    Pending,
    /// Downloading transcript from video source
    Downloading,
    /// AI is analyzing transcript for highlights
    Analyzing,
    /// Analysis completed successfully
    Completed,
    /// Analysis failed
    Failed,
    /// Draft has expired (TTL exceeded)
    Expired,
}

impl AnalysisStatus {
    /// Returns the status as a string for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Analyzing => "analyzing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    /// Returns true if the status is terminal (completed, failed, or expired).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Expired)
    }

    /// Returns true if the status indicates the job is still in progress.
    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::Pending | Self::Downloading | Self::Analyzing)
    }
}

/// An analysis draft representing a video that has been analyzed.
///
/// This is the persistent record of an analysis job. Once analysis completes,
/// the user can select scenes and styles to process.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisDraft {
    /// Unique identifier for this draft (UUID)
    pub id: String,

    /// User who owns this draft
    pub user_id: String,

    /// Source video URL (YouTube, etc.)
    pub source_url: String,

    /// Video title extracted from source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,

    /// Optional AI instructions from user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_instructions: Option<String>,

    /// Current status of the analysis
    pub status: AnalysisStatus,

    /// Sanitized error message for user display (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Request ID for support debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Number of scenes successfully parsed
    pub scene_count: u32,

    /// Number of scenes that had parsing warnings
    pub warning_count: u32,

    /// When the draft was created
    pub created_at: DateTime<Utc>,

    /// When the draft was last updated
    pub updated_at: DateTime<Utc>,

    /// When the draft expires (TTL)
    pub expires_at: DateTime<Utc>,
}

impl AnalysisDraft {
    /// Create a new analysis draft with pending status.
    pub fn new(
        id: impl Into<String>,
        user_id: impl Into<String>,
        source_url: impl Into<String>,
        ttl_days: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            user_id: user_id.into(),
            source_url: source_url.into(),
            video_title: None,
            prompt_instructions: None,
            status: AnalysisStatus::Pending,
            error_message: None,
            request_id: None,
            scene_count: 0,
            warning_count: 0,
            created_at: now,
            updated_at: now,
            expires_at: now + chrono::Duration::days(ttl_days),
        }
    }

    /// Set the video title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.video_title = Some(title.into());
        self
    }

    /// Set the prompt instructions.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt_instructions = Some(prompt.into());
        self
    }

    /// Set the request ID.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Check if the draft has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// A scene within an analysis draft.
///
/// This represents a single highlight/scene detected in the video.
/// The fields match the existing `Highlight` type but are scoped to drafts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DraftScene {
    /// Scene index (1-based, matches highlight ID)
    pub id: u32,

    /// Parent draft ID
    pub analysis_draft_id: String,

    /// Scene title
    pub title: String,

    /// Description of the scene
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Reason why this is a good clip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Start timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub start: String,

    /// End timestamp (HH:MM:SS or HH:MM:SS.mmm)
    pub end: String,

    /// Duration in seconds
    pub duration_secs: u32,

    /// Padding before the start timestamp (seconds)
    #[serde(default = "default_pad")]
    pub pad_before: f64,

    /// Padding after the end timestamp (seconds)
    #[serde(default = "default_pad")]
    pub pad_after: f64,

    /// Confidence score (0.0-1.0) if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    /// Hook category (emotional, educational, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_category: Option<String>,
}

fn default_pad() -> f64 {
    1.0
}

impl DraftScene {
    /// Create a new draft scene.
    pub fn new(
        id: u32,
        analysis_draft_id: impl Into<String>,
        title: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
        duration_secs: u32,
    ) -> Self {
        Self {
            id,
            analysis_draft_id: analysis_draft_id.into(),
            title: title.into(),
            description: None,
            reason: None,
            start: start.into(),
            end: end.into(),
            duration_secs,
            pad_before: 1.0,
            pad_after: 1.0,
            confidence: None,
            hook_category: None,
        }
    }

    /// Create from an existing Highlight.
    pub fn from_highlight(
        analysis_draft_id: impl Into<String>,
        highlight: &crate::Highlight,
    ) -> Self {
        Self {
            id: highlight.id,
            analysis_draft_id: analysis_draft_id.into(),
            title: highlight.title.clone(),
            description: highlight.description.clone(),
            reason: highlight.reason.clone(),
            start: highlight.start.clone(),
            end: highlight.end.clone(),
            duration_secs: highlight.duration,
            pad_before: highlight.pad_before,
            pad_after: highlight.pad_after,
            confidence: None, // Highlights don't have confidence yet
            hook_category: highlight
                .hook_category
                .as_ref()
                .map(|c| format!("{:?}", c).to_lowercase()),
        }
    }
}

/// Per-scene selection with render toggles.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneSelection {
    /// Scene ID to process
    pub scene_id: u32,

    /// Whether to render in FULL (single panel) mode
    pub render_full: bool,

    /// Whether to render in SPLIT (dual panel) mode
    pub render_split: bool,
}

/// Request to process selected scenes from a draft.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessDraftRequest {
    /// The draft to process from
    pub analysis_draft_id: String,

    /// Selected scenes with render toggles
    pub selected_scenes: Vec<SceneSelection>,

    /// Style to use for FULL renders (e.g., "intelligent", "center_focus")
    pub full_style: String,

    /// Style to use for SPLIT renders (e.g., "intelligent_split", "split_fast")
    pub split_style: String,

    /// Client-generated idempotency key
    pub idempotency_key: String,
}

impl ProcessDraftRequest {
    /// Count total render jobs that would be created.
    pub fn total_jobs(&self) -> usize {
        self.selected_scenes
            .iter()
            .map(|s| (s.render_full as usize) + (s.render_split as usize))
            .sum()
    }

    /// Validate the request.
    pub fn validate(&self) -> Result<(), String> {
        if self.selected_scenes.is_empty() {
            return Err("At least one scene must be selected".to_string());
        }

        if self.full_style.is_empty() {
            return Err("Full style must be specified".to_string());
        }

        if self.split_style.is_empty() {
            return Err("Split style must be specified".to_string());
        }

        if self.idempotency_key.is_empty() {
            return Err("Idempotency key is required".to_string());
        }

        // Check that at least one scene has a render toggle enabled
        let has_renders = self
            .selected_scenes
            .iter()
            .any(|s| s.render_full || s.render_split);

        if !has_renders {
            return Err("At least one render toggle must be enabled".to_string());
        }

        Ok(())
    }

    /// Get the idempotency key for this request.
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}

/// Response from starting an analysis job.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartAnalysisResponse {
    /// Job ID for polling status
    pub job_id: String,

    /// Draft ID that will be created
    pub draft_id: String,
}

/// Response from polling analysis status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisStatusResponse {
    /// Current status
    pub status: AnalysisStatus,

    /// Draft ID (if analysis has started)
    pub draft_id: String,

    /// Video title (if extracted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_title: Option<String>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Scene count (if completed)
    pub scene_count: u32,

    /// Warning count (if completed)
    pub warning_count: u32,
}

/// Processing cost estimate.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessingEstimate {
    /// Number of scenes selected
    pub scene_count: u32,

    /// Total duration of selected scenes (seconds)
    pub total_duration_secs: u32,

    /// Estimated credits required
    pub estimated_credits: u32,

    /// Estimated processing time in seconds (lower bound)
    pub estimated_time_min_secs: u32,

    /// Estimated processing time in seconds (upper bound)
    pub estimated_time_max_secs: u32,

    /// Number of FULL renders
    pub full_render_count: u32,

    /// Number of SPLIT renders
    pub split_render_count: u32,

    /// Whether user would exceed quota
    pub exceeds_quota: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_status_is_terminal() {
        assert!(!AnalysisStatus::Pending.is_terminal());
        assert!(!AnalysisStatus::Downloading.is_terminal());
        assert!(!AnalysisStatus::Analyzing.is_terminal());
        assert!(AnalysisStatus::Completed.is_terminal());
        assert!(AnalysisStatus::Failed.is_terminal());
        assert!(AnalysisStatus::Expired.is_terminal());
    }

    #[test]
    fn test_draft_expiry() {
        let draft = AnalysisDraft::new("test-id", "user-id", "https://youtube.com/watch?v=test", 7);
        assert!(!draft.is_expired());
        assert!(draft.expires_at > Utc::now());
    }

    #[test]
    fn test_process_request_validation() {
        let valid_request = ProcessDraftRequest {
            analysis_draft_id: "draft-1".to_string(),
            selected_scenes: vec![SceneSelection {
                scene_id: 1,
                render_full: true,
                render_split: false,
            }],
            full_style: "intelligent".to_string(),
            split_style: "intelligent_split".to_string(),
            idempotency_key: "key-123".to_string(),
        };
        assert!(valid_request.validate().is_ok());
        assert_eq!(valid_request.total_jobs(), 1);

        let empty_scenes = ProcessDraftRequest {
            selected_scenes: vec![],
            ..valid_request.clone()
        };
        assert!(empty_scenes.validate().is_err());

        let no_renders = ProcessDraftRequest {
            selected_scenes: vec![SceneSelection {
                scene_id: 1,
                render_full: false,
                render_split: false,
            }],
            ..valid_request.clone()
        };
        assert!(no_renders.validate().is_err());
    }
}
