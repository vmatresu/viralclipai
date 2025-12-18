//! Credit transaction data models.
//!
//! This module provides types for tracking credit usage history.
//! Each credit transaction records when credits were charged,
//! for what operation, and the resulting balance.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Type of credit operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreditOperationType {
    /// Video analysis (scene detection)
    Analysis,
    /// Initial scene processing/rendering
    SceneProcessing,
    /// Re-processing existing scenes with new styles
    Reprocessing,
    /// Silent remover add-on
    SilentRemover,
    /// Object detection add-on
    ObjectDetection,
    /// Scene originals download
    SceneOriginals,
    /// Manual admin adjustment (refund, correction, etc.)
    AdminAdjustment,
}

impl CreditOperationType {
    /// Returns the operation type as a string for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::SceneProcessing => "scene_processing",
            Self::Reprocessing => "reprocessing",
            Self::SilentRemover => "silent_remover",
            Self::ObjectDetection => "object_detection",
            Self::SceneOriginals => "scene_originals",
            Self::AdminAdjustment => "admin_adjustment",
        }
    }

    /// Returns a human-readable label for the operation type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Analysis => "Video Analysis",
            Self::SceneProcessing => "Scene Processing",
            Self::Reprocessing => "Reprocessing",
            Self::SilentRemover => "Silent Remover",
            Self::ObjectDetection => "Object Detection",
            Self::SceneOriginals => "Scene Originals",
            Self::AdminAdjustment => "Admin Adjustment",
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "analysis" => Some(Self::Analysis),
            "scene_processing" => Some(Self::SceneProcessing),
            "reprocessing" => Some(Self::Reprocessing),
            "silent_remover" => Some(Self::SilentRemover),
            "object_detection" => Some(Self::ObjectDetection),
            "scene_originals" => Some(Self::SceneOriginals),
            "admin_adjustment" => Some(Self::AdminAdjustment),
            _ => None,
        }
    }
}

/// A credit transaction record.
///
/// Each time credits are charged, a transaction is recorded with the
/// operation details and resulting balance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreditTransaction {
    /// Unique identifier for this transaction (UUID)
    pub id: String,

    /// User who was charged
    pub user_id: String,

    /// When the transaction occurred
    pub timestamp: DateTime<Utc>,

    /// Type of operation that consumed credits
    pub operation_type: CreditOperationType,

    /// Number of credits charged
    pub credits_amount: u32,

    /// Human-readable description of the operation
    pub description: String,

    /// Credits used this month after this transaction
    pub balance_after: u32,

    /// Associated video ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,

    /// Associated draft ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,

    /// Additional metadata (e.g., style names, scene count)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    /// When the record was created (same as timestamp for new transactions)
    pub created_at: DateTime<Utc>,
}

impl CreditTransaction {
    /// Create a new credit transaction.
    pub fn new(
        id: String,
        user_id: String,
        operation_type: CreditOperationType,
        credits_amount: u32,
        description: String,
        balance_after: u32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            user_id,
            timestamp: now,
            operation_type,
            credits_amount,
            description,
            balance_after,
            video_id: None,
            draft_id: None,
            metadata: None,
            created_at: now,
        }
    }

    /// Set the video ID.
    pub fn with_video_id(mut self, video_id: impl Into<String>) -> Self {
        self.video_id = Some(video_id.into());
        self
    }

    /// Set the draft ID.
    pub fn with_draft_id(mut self, draft_id: impl Into<String>) -> Self {
        self.draft_id = Some(draft_id.into());
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set video ID if Some, otherwise no-op.
    pub fn with_optional_video_id(mut self, video_id: Option<String>) -> Self {
        if let Some(vid) = video_id {
            self.video_id = Some(vid);
        }
        self
    }

    /// Set draft ID if Some, otherwise no-op.
    pub fn with_optional_draft_id(mut self, draft_id: Option<String>) -> Self {
        if let Some(did) = draft_id {
            self.draft_id = Some(did);
        }
        self
    }

    /// Set metadata if Some, otherwise no-op.
    pub fn with_optional_metadata(mut self, metadata: Option<HashMap<String, String>>) -> Self {
        if let Some(meta) = metadata {
            self.metadata = Some(meta);
        }
        self
    }
}

/// Context for recording a credit transaction.
///
/// This is passed alongside credit reservation requests to capture
/// what operation the credits are being used for.
#[derive(Debug, Clone)]
pub struct CreditContext {
    /// Type of operation
    pub operation_type: CreditOperationType,

    /// Human-readable description
    pub description: String,

    /// Associated video ID (if any)
    pub video_id: Option<String>,

    /// Associated draft ID (if any)
    pub draft_id: Option<String>,

    /// Additional metadata
    pub metadata: Option<HashMap<String, String>>,
}

impl CreditContext {
    /// Create a new credit context.
    pub fn new(operation_type: CreditOperationType, description: impl Into<String>) -> Self {
        Self {
            operation_type,
            description: description.into(),
            video_id: None,
            draft_id: None,
            metadata: None,
        }
    }

    /// Set the video ID.
    pub fn with_video_id(mut self, video_id: impl Into<String>) -> Self {
        self.video_id = Some(video_id.into());
        self
    }

    /// Set the draft ID.
    pub fn with_draft_id(mut self, draft_id: impl Into<String>) -> Self {
        self.draft_id = Some(draft_id.into());
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}
