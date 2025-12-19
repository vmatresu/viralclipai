//! Video sorting and pagination utilities.
//!
//! Provides type-safe sorting configuration and cursor-based pagination
//! for Firestore queries.

use crate::types::{CollectionSelector, Cursor, FieldReference, Order, StructuredQuery, Value};

// ============================================================================
// Sort Configuration
// ============================================================================

/// Supported sort fields for video queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoSortField {
    /// Sort by creation date (default)
    #[default]
    CreatedAt,
    /// Sort by video title (case-sensitive)
    Title,
    /// Sort by processing status
    Status,
    /// Sort by total file size
    Size,
}

impl VideoSortField {
    /// Parse from string, returning default if invalid.
    pub fn from_str_or_default(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "date" | "created_at" => Self::CreatedAt,
            "title" | "video_title" => Self::Title,
            "status" => Self::Status,
            "size" | "total_size_bytes" => Self::Size,
            _ => Self::CreatedAt,
        }
    }

    /// Get the Firestore field path for this sort field.
    pub const fn firestore_field(&self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::Title => "video_title",
            Self::Status => "status",
            Self::Size => "total_size_bytes",
        }
    }

    /// Convert a value to Firestore Value type for cursor.
    pub fn to_cursor_value(&self, value: &str) -> Value {
        match self {
            Self::CreatedAt => Value::TimestampValue(value.to_string()),
            Self::Size => Value::IntegerValue(value.to_string()),
            Self::Title | Self::Status => Value::StringValue(value.to_string()),
        }
    }
}

/// Sort direction for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    Ascending,
    #[default]
    Descending,
}

impl SortDirection {
    /// Parse from string, returning default if invalid.
    pub fn from_str_or_default(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Self::Ascending,
            _ => Self::Descending,
        }
    }

    /// Get the Firestore direction string.
    pub const fn firestore_direction(&self) -> &'static str {
        match self {
            Self::Ascending => "ASCENDING",
            Self::Descending => "DESCENDING",
        }
    }
}

/// Complete sort configuration.
#[derive(Debug, Clone)]
pub struct SortConfig {
    pub field: VideoSortField,
    pub direction: SortDirection,
}

impl Default for SortConfig {
    fn default() -> Self {
        Self {
            field: VideoSortField::CreatedAt,
            direction: SortDirection::Descending,
        }
    }
}

impl SortConfig {
    /// Create a new sort configuration.
    pub fn new(field: VideoSortField, direction: SortDirection) -> Self {
        Self { field, direction }
    }

    /// Create from string parameters with validation.
    pub fn from_params(field: Option<&str>, direction: Option<&str>) -> Self {
        Self {
            field: field.map(VideoSortField::from_str_or_default).unwrap_or_default(),
            direction: direction.map(SortDirection::from_str_or_default).unwrap_or_default(),
        }
    }
}

// ============================================================================
// Pagination Cursor
// ============================================================================

/// Separator used in cursor encoding.
const CURSOR_SEPARATOR: &str = "|";

/// Pagination cursor for sorted queries.
#[derive(Debug, Clone)]
pub struct PaginationCursor {
    /// The sort field value at the cursor position.
    pub sort_value: String,
    /// The document reference path.
    pub doc_path: String,
}

impl PaginationCursor {
    /// Create a new cursor.
    pub fn new(sort_value: impl Into<String>, doc_path: impl Into<String>) -> Self {
        Self {
            sort_value: sort_value.into(),
            doc_path: doc_path.into(),
        }
    }

    /// Encode cursor to a URL-safe string.
    pub fn encode(&self) -> String {
        let raw = format!("{}{}{}", self.sort_value, CURSOR_SEPARATOR, self.doc_path);
        urlencoding::encode(&raw).into_owned()
    }

    /// Decode cursor from URL-encoded string.
    pub fn decode(encoded: &str) -> Option<Self> {
        let decoded = urlencoding::decode(encoded).ok()?;
        let (sort_value, doc_path) = decoded.split_once(CURSOR_SEPARATOR)?;

        // Validate doc_path looks like a Firestore document reference
        if !doc_path.contains("/documents/") {
            return None;
        }

        Some(Self {
            sort_value: sort_value.to_string(),
            doc_path: doc_path.to_string(),
        })
    }

    /// Build cursor for a video document.
    pub fn for_video(
        project_id: &str,
        user_id: &str,
        video_id: &str,
        sort_value: &str,
    ) -> Self {
        let doc_path = format!(
            "projects/{}/databases/(default)/documents/users/{}/videos/{}",
            project_id, user_id, video_id
        );
        Self::new(sort_value, doc_path)
    }
}

// ============================================================================
// Query Builder
// ============================================================================

/// Pagination limits.
pub const DEFAULT_PAGE_SIZE: u32 = 25;
pub const MIN_PAGE_SIZE: u32 = 1;
pub const MAX_PAGE_SIZE: u32 = 100;

/// Normalize page size to valid range.
pub fn normalize_page_size(limit: Option<u32>) -> i32 {
    limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(MIN_PAGE_SIZE, MAX_PAGE_SIZE) as i32
}

/// Build a sorted Firestore query for videos.
pub fn build_sorted_video_query(
    collection_id: &str,
    sort: &SortConfig,
    limit: i32,
    cursor: Option<&PaginationCursor>,
) -> StructuredQuery {
    let direction = sort.direction.firestore_direction().to_string();

    let mut query = StructuredQuery {
        from: vec![CollectionSelector {
            collection_id: collection_id.to_string(),
            all_descendants: None,
        }],
        r#where: None,
        order_by: Some(vec![
            // Primary sort field
            Order {
                field: FieldReference {
                    field_path: sort.field.firestore_field().to_string(),
                },
                direction: direction.clone(),
            },
            // Secondary sort by document ID for stable pagination
            Order {
                field: FieldReference {
                    field_path: "__name__".to_string(),
                },
                direction,
            },
        ]),
        start_at: None,
        limit: Some(limit),
    };

    // Add cursor for pagination
    if let Some(c) = cursor {
        query.start_at = Some(Cursor {
            values: vec![
                sort.field.to_cursor_value(&c.sort_value),
                Value::ReferenceValue(c.doc_path.clone()),
            ],
            before: Some(false), // Start just after this position
        });
    }

    query
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_field_parsing() {
        assert_eq!(
            VideoSortField::from_str_or_default("date"),
            VideoSortField::CreatedAt
        );
        assert_eq!(
            VideoSortField::from_str_or_default("title"),
            VideoSortField::Title
        );
        assert_eq!(
            VideoSortField::from_str_or_default("invalid"),
            VideoSortField::CreatedAt
        );
    }

    #[test]
    fn test_sort_direction_parsing() {
        assert_eq!(
            SortDirection::from_str_or_default("asc"),
            SortDirection::Ascending
        );
        assert_eq!(
            SortDirection::from_str_or_default("desc"),
            SortDirection::Descending
        );
        assert_eq!(
            SortDirection::from_str_or_default("invalid"),
            SortDirection::Descending
        );
    }

    #[test]
    fn test_cursor_encode_decode() {
        let cursor = PaginationCursor::new(
            "2024-01-01T00:00:00Z",
            "projects/test/databases/(default)/documents/users/123/videos/abc",
        );

        let encoded = cursor.encode();
        let decoded = PaginationCursor::decode(&encoded).unwrap();

        assert_eq!(decoded.sort_value, cursor.sort_value);
        assert_eq!(decoded.doc_path, cursor.doc_path);
    }

    #[test]
    fn test_cursor_decode_invalid() {
        assert!(PaginationCursor::decode("invalid").is_none());
        assert!(PaginationCursor::decode("value|invalid_path").is_none());
    }
}
