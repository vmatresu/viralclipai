//! Firestore REST API types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Firestore document value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Value {
    NullValue(()),
    BooleanValue(bool),
    IntegerValue(String), // Firestore sends integers as strings
    DoubleValue(f64),
    TimestampValue(String),
    StringValue(String),
    BytesValue(String),
    ReferenceValue(String),
    GeoPointValue(GeoPoint),
    ArrayValue(ArrayValue),
    MapValue(MapValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayValue {
    pub values: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapValue {
    pub fields: Option<HashMap<String, Value>>,
}

/// Firestore document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Full resource name
    pub name: Option<String>,
    /// Document fields
    pub fields: Option<HashMap<String, Value>>,
    /// Create time
    pub create_time: Option<String>,
    /// Update time
    pub update_time: Option<String>,
}

impl Document {
    /// Create a new document with the given fields.
    pub fn new(fields: HashMap<String, Value>) -> Self {
        Self {
            name: None,
            fields: Some(fields),
            create_time: None,
            update_time: None,
        }
    }
}

/// Request to create a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDocumentRequest {
    pub fields: HashMap<String, Value>,
}

/// Request to update a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentRequest {
    pub fields: HashMap<String, Value>,
}

/// List documents response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDocumentsResponse {
    pub documents: Option<Vec<Document>>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchGetDocumentsRequest {
    pub documents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<DocumentMask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchGetDocumentsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<Document>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
 }

// ============================================================================
// Batch Write Types (for atomic multi-document operations)
// ============================================================================

/// A single write operation in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Write {
    /// Update or insert a document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<Document>,

    /// Delete a document by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,

    /// Field mask for partial updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<DocumentMask>,

    /// Precondition for the write.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_document: Option<Precondition>,
}

/// Document field mask for partial updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMask {
    pub field_paths: Vec<String>,
}

/// Precondition for a write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Precondition {
    /// Document must exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,

    /// Document must have this update time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
}

/// Batch write request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchWriteRequest {
    pub writes: Vec<Write>,
}

/// Result of a single write in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    /// Update time of the written document.
    pub update_time: Option<String>,
}

/// Status of a single write in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// gRPC status code (0 = OK).
    pub code: Option<i32>,
    /// Error message if failed.
    pub message: Option<String>,
}

/// Batch write response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchWriteResponse {
    /// Results for each write, in order.
    pub write_results: Option<Vec<WriteResult>>,
    /// Status for each write, in order.
    pub status: Option<Vec<Status>>,
}

impl BatchWriteResponse {
    /// Create an empty response for empty batch writes.
    pub fn empty() -> Self {
        Self {
            write_results: Some(vec![]),
            status: Some(vec![]),
        }
    }

    /// Check for partial failures in the batch response.
    pub fn check_for_errors(&self) -> crate::error::FirestoreResult<()> {
        if let Some(statuses) = &self.status {
            for (i, status) in statuses.iter().enumerate() {
                if let Some(code) = status.code {
                    if code != 0 {
                        let msg = status.message.as_deref().unwrap_or("Unknown error");
                        return Err(crate::error::FirestoreError::request_failed(format!(
                            "Batch write failed at index {}: {} (code {})",
                            i, msg, code
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Convert a Rust value to Firestore Value.
pub trait ToFirestoreValue {
    fn to_firestore_value(&self) -> Value;
}

impl ToFirestoreValue for String {
    fn to_firestore_value(&self) -> Value {
        Value::StringValue(self.clone())
    }
}

impl ToFirestoreValue for &str {
    fn to_firestore_value(&self) -> Value {
        Value::StringValue(self.to_string())
    }
}

impl ToFirestoreValue for i64 {
    fn to_firestore_value(&self) -> Value {
        Value::IntegerValue(self.to_string())
    }
}

impl ToFirestoreValue for i32 {
    fn to_firestore_value(&self) -> Value {
        Value::IntegerValue((*self as i64).to_string())
    }
}

impl ToFirestoreValue for u32 {
    fn to_firestore_value(&self) -> Value {
        Value::IntegerValue((*self as i64).to_string())
    }
}

impl ToFirestoreValue for u64 {
    fn to_firestore_value(&self) -> Value {
        Value::IntegerValue((*self as i64).to_string())
    }
}

impl ToFirestoreValue for f64 {
    fn to_firestore_value(&self) -> Value {
        Value::DoubleValue(*self)
    }
}

impl ToFirestoreValue for bool {
    fn to_firestore_value(&self) -> Value {
        Value::BooleanValue(*self)
    }
}

impl ToFirestoreValue for DateTime<Utc> {
    fn to_firestore_value(&self) -> Value {
        Value::TimestampValue(self.to_rfc3339())
    }
}

impl<T: ToFirestoreValue> ToFirestoreValue for Option<T> {
    fn to_firestore_value(&self) -> Value {
        match self {
            Some(v) => v.to_firestore_value(),
            None => Value::NullValue(()),
        }
    }
}

impl<T: ToFirestoreValue> ToFirestoreValue for Vec<T> {
    fn to_firestore_value(&self) -> Value {
        Value::ArrayValue(ArrayValue {
            values: Some(self.iter().map(|v| v.to_firestore_value()).collect()),
        })
    }
}

impl<T: ToFirestoreValue> ToFirestoreValue for HashMap<String, T> {
    fn to_firestore_value(&self) -> Value {
        Value::MapValue(MapValue {
            fields: Some(
                self.iter()
                    .map(|(k, v)| (k.clone(), v.to_firestore_value()))
                    .collect(),
            ),
        })
    }
}

/// Convert Firestore Value to Rust type.
pub trait FromFirestoreValue: Sized {
    fn from_firestore_value(value: &Value) -> Option<Self>;
}

impl FromFirestoreValue for String {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::StringValue(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl FromFirestoreValue for i64 {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::IntegerValue(s) => s.parse().ok(),
            Value::DoubleValue(f) => Some(*f as i64),
            _ => None,
        }
    }
}

impl FromFirestoreValue for u32 {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::IntegerValue(s) => s.parse().ok(),
            Value::DoubleValue(f) => Some(*f as u32),
            _ => None,
        }
    }
}

impl FromFirestoreValue for u64 {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::IntegerValue(s) => s.parse().ok(),
            Value::DoubleValue(f) => Some(*f as u64),
            _ => None,
        }
    }
}

impl FromFirestoreValue for f64 {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::DoubleValue(f) => Some(*f),
            Value::IntegerValue(s) => s.parse().ok(),
            _ => None,
        }
    }
}

impl FromFirestoreValue for bool {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::BooleanValue(b) => Some(*b),
            _ => None,
        }
    }
}

impl FromFirestoreValue for DateTime<Utc> {
    fn from_firestore_value(value: &Value) -> Option<Self> {
        match value {
            Value::TimestampValue(s) => DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.into()),
            _ => None,
        }
    }
}
