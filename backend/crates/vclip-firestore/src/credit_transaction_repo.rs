//! Credit transaction repository for tracking credit usage in Firestore.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use vclip_models::{CreditOperationType, CreditTransaction};

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{
    CollectionSelector, Cursor, FieldFilter, FieldReference, Filter, FromFirestoreValue, Order,
    StructuredQuery, ToFirestoreValue, Value,
};

/// Repository for credit transaction documents.
pub struct CreditTransactionRepository {
    client: FirestoreClient,
    user_id: String,
}

impl CreditTransactionRepository {
    /// Create a new credit transaction repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Collection path for user's credit transactions.
    fn collection(&self) -> String {
        format!("users/{}/credit_transactions", self.user_id)
    }

    /// Create a new credit transaction record.
    pub async fn create(&self, transaction: &CreditTransaction) -> FirestoreResult<()> {
        let fields = transaction_to_fields(transaction);
        self.client
            .create_document(&self.collection(), &transaction.id, fields)
            .await?;
        info!(
            "Created credit transaction {} for user {} ({} credits for {})",
            transaction.id, self.user_id, transaction.credits_amount, transaction.operation_type.as_str()
        );
        Ok(())
    }

    /// Get a credit transaction by ID.
    pub async fn get(&self, transaction_id: &str) -> FirestoreResult<Option<CreditTransaction>> {
        let doc = self
            .client
            .get_document(&self.collection(), transaction_id)
            .await?;

        match doc {
            Some(d) => {
                let transaction = document_to_transaction(&d, transaction_id)?;
                Ok(Some(transaction))
            }
            None => Ok(None),
        }
    }

    /// List credit transactions with pagination.
    ///
    /// Returns transactions ordered by timestamp (newest first) with optional filtering.
    /// Uses Firestore runQuery for proper server-side ordering and pagination.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of transactions to return (clamped to 1..=100)
    /// * `cursor_timestamp` - ISO8601 timestamp to start after (for pagination)
    /// * `operation_type` - Optional filter by operation type
    pub async fn list_page(
        &self,
        limit: Option<u32>,
        cursor_timestamp: Option<&str>,
        operation_type: Option<&str>,
    ) -> FirestoreResult<(Vec<CreditTransaction>, Option<String>)> {
        // Clamp limit to safe range
        let effective_limit = limit.unwrap_or(50).clamp(1, 100) as i32;

        // Build structured query
        let mut query = StructuredQuery {
            from: vec![CollectionSelector {
                collection_id: "credit_transactions".to_string(),
                all_descendants: None,
            }],
            r#where: None,
            order_by: Some(vec![Order {
                field: FieldReference {
                    field_path: "timestamp".to_string(),
                },
                direction: "DESCENDING".to_string(),
            }]),
            start_at: None,
            limit: Some(effective_limit),
        };

        // Add operation_type filter if specified
        if let Some(op_type) = operation_type {
            query.r#where = Some(Filter {
                composite_filter: None,
                field_filter: Some(FieldFilter {
                    field: FieldReference {
                        field_path: "operation_type".to_string(),
                    },
                    op: "EQUAL".to_string(),
                    value: Value::StringValue(op_type.to_string()),
                }),
            });
        }

        // Add cursor for pagination (start after the given timestamp)
        if let Some(ts) = cursor_timestamp {
            query.start_at = Some(Cursor {
                values: vec![Value::TimestampValue(ts.to_string())],
                before: Some(false), // Start just after this position
            });
        }

        // Run the query
        let parent_path = format!("users/{}", self.user_id);
        let docs = self.client.run_query(&parent_path, query).await?;

        let mut transactions = Vec::new();
        let mut parse_errors = 0u32;

        for doc in docs {
            if let Some(name) = &doc.name {
                let tx_id = name.split('/').last().unwrap_or("").to_string();
                match document_to_transaction(&doc, &tx_id) {
                    Ok(tx) => transactions.push(tx),
                    Err(e) => {
                        // Fix #2: Log parse failures instead of silently dropping
                        warn!(
                            user_id = %self.user_id,
                            tx_id = %tx_id,
                            error = %e,
                            "Failed to parse credit transaction document"
                        );
                        parse_errors += 1;
                    }
                }
            }
        }

        if parse_errors > 0 {
            warn!(
                user_id = %self.user_id,
                parse_errors = parse_errors,
                "Some credit transactions failed to parse"
            );
        }

        // Generate next page cursor from last transaction's timestamp
        let next_cursor = transactions.last().map(|tx| tx.timestamp.to_rfc3339());

        Ok((transactions, next_cursor))
    }

    /// List credit transactions with pagination (legacy signature for backward compatibility).
    ///
    /// Deprecated: Use list_page with operation_type filter instead.
    pub async fn list_page_legacy(
        &self,
        limit: Option<u32>,
        _page_token: Option<&str>,
    ) -> FirestoreResult<(Vec<CreditTransaction>, Option<String>)> {
        // Note: Old page_token is incompatible with new cursor-based pagination
        // This method exists for backward compatibility during migration
        self.list_page(limit, None, None).await
    }

    /// List all credit transactions for the user.
    pub async fn list(&self, limit: Option<u32>) -> FirestoreResult<Vec<CreditTransaction>> {
        let (transactions, _) = self.list_page(limit, None, None).await?;
        Ok(transactions)
    }

    /// Count total credits used in a given month.
    ///
    /// Month key format: "YYYY-MM" (e.g., "2025-01")
    /// Uses timestamp range query for efficiency instead of O(N) client-side filtering.
    pub async fn get_month_total(&self, month_key: &str) -> FirestoreResult<u32> {
        let transactions = self.list_month_transactions(month_key).await?;
        Ok(transactions.iter().map(|tx| tx.credits_amount).sum())
    }

    /// Get summary of credits by operation type for a given month.
    /// Uses timestamp range query for efficiency instead of O(N) client-side filtering.
    pub async fn get_month_summary(
        &self,
        month_key: &str,
    ) -> FirestoreResult<HashMap<String, u32>> {
        let transactions = self.list_month_transactions(month_key).await?;

        let mut by_operation: HashMap<String, u32> = HashMap::new();
        for tx in transactions {
            *by_operation
                .entry(tx.operation_type.as_str().to_string())
                .or_insert(0) += tx.credits_amount;
        }

        Ok(by_operation)
    }

    /// List all transactions for a specific month using timestamp range query.
    ///
    /// Month key format: "YYYY-MM" (e.g., "2025-01")
    async fn list_month_transactions(
        &self,
        month_key: &str,
    ) -> FirestoreResult<Vec<CreditTransaction>> {
        // Parse month key to get start and end timestamps
        let (start_ts, end_ts) = parse_month_range(month_key)?;

        // Build structured query with timestamp range filter
        let query = StructuredQuery {
            from: vec![CollectionSelector {
                collection_id: "credit_transactions".to_string(),
                all_descendants: None,
            }],
            r#where: Some(Filter {
                composite_filter: Some(crate::types::CompositeFilter {
                    op: "AND".to_string(),
                    filters: vec![
                        Filter {
                            composite_filter: None,
                            field_filter: Some(FieldFilter {
                                field: FieldReference {
                                    field_path: "timestamp".to_string(),
                                },
                                op: "GREATER_THAN_OR_EQUAL".to_string(),
                                value: Value::TimestampValue(start_ts),
                            }),
                        },
                        Filter {
                            composite_filter: None,
                            field_filter: Some(FieldFilter {
                                field: FieldReference {
                                    field_path: "timestamp".to_string(),
                                },
                                op: "LESS_THAN".to_string(),
                                value: Value::TimestampValue(end_ts),
                            }),
                        },
                    ],
                }),
                field_filter: None,
            }),
            order_by: Some(vec![Order {
                field: FieldReference {
                    field_path: "timestamp".to_string(),
                },
                direction: "DESCENDING".to_string(),
            }]),
            start_at: None,
            limit: Some(1000), // Reasonable max for a single month
        };

        let parent_path = format!("users/{}", self.user_id);
        let docs = self.client.run_query(&parent_path, query).await?;

        let mut transactions = Vec::new();
        for doc in docs {
            if let Some(name) = &doc.name {
                let tx_id = name.split('/').last().unwrap_or("").to_string();
                match document_to_transaction(&doc, &tx_id) {
                    Ok(tx) => transactions.push(tx),
                    Err(e) => {
                        warn!(
                            user_id = %self.user_id,
                            tx_id = %tx_id,
                            error = %e,
                            "Failed to parse credit transaction in month query"
                        );
                    }
                }
            }
        }

        Ok(transactions)
    }
}

/// Parse a month key (YYYY-MM) into start and end RFC3339 timestamps.
fn parse_month_range(month_key: &str) -> FirestoreResult<(String, String)> {
    let parts: Vec<&str> = month_key.split('-').collect();
    if parts.len() != 2 {
        return Err(FirestoreError::InvalidResponse(format!(
            "Invalid month key format: {}",
            month_key
        )));
    }

    let year: i32 = parts[0].parse().map_err(|_| {
        FirestoreError::InvalidResponse(format!("Invalid year in month key: {}", month_key))
    })?;
    let month: u32 = parts[1].parse().map_err(|_| {
        FirestoreError::InvalidResponse(format!("Invalid month in month key: {}", month_key))
    })?;

    if !(1..=12).contains(&month) {
        return Err(FirestoreError::InvalidResponse(format!(
            "Month out of range: {}",
            month_key
        )));
    }

    // Start of month
    let start = DateTime::<Utc>::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| FirestoreError::InvalidResponse("Invalid date".to_string()))?
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Utc,
    );

    // Start of next month
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = DateTime::<Utc>::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
            .ok_or_else(|| FirestoreError::InvalidResponse("Invalid date".to_string()))?
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Utc,
    );

    Ok((start.to_rfc3339(), end.to_rfc3339()))
}

// Helper functions for conversion

fn transaction_to_fields(tx: &CreditTransaction) -> HashMap<String, Value> {
    let mut fields = HashMap::new();

    fields.insert("id".to_string(), tx.id.to_firestore_value());
    fields.insert("user_id".to_string(), tx.user_id.to_firestore_value());
    fields.insert("timestamp".to_string(), tx.timestamp.to_firestore_value());
    fields.insert(
        "operation_type".to_string(),
        tx.operation_type.as_str().to_firestore_value(),
    );
    fields.insert(
        "credits_amount".to_string(),
        tx.credits_amount.to_firestore_value(),
    );
    fields.insert(
        "description".to_string(),
        tx.description.to_firestore_value(),
    );
    fields.insert(
        "balance_after".to_string(),
        tx.balance_after.to_firestore_value(),
    );

    if let Some(ref video_id) = tx.video_id {
        fields.insert("video_id".to_string(), video_id.to_firestore_value());
    }

    if let Some(ref draft_id) = tx.draft_id {
        fields.insert("draft_id".to_string(), draft_id.to_firestore_value());
    }

    if let Some(ref metadata) = tx.metadata {
        // Store metadata as a map value
        let map_fields: HashMap<String, Value> = metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.to_firestore_value()))
            .collect();
        fields.insert(
            "metadata".to_string(),
            Value::MapValue(crate::types::MapValue {
                fields: Some(map_fields),
            }),
        );
    }

    fields.insert("created_at".to_string(), tx.created_at.to_firestore_value());

    fields
}

fn document_to_transaction(
    doc: &crate::types::Document,
    tx_id: &str,
) -> FirestoreResult<CreditTransaction> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse(format!(
            "Transaction {} has no fields",
            tx_id
        ))
    })?;

    let user_id = fields
        .get("user_id")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let timestamp = fields
        .get("timestamp")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(chrono::Utc::now);

    let operation_type_str = fields
        .get("operation_type")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_else(|| "scene_processing".to_string());

    let operation_type =
        CreditOperationType::from_str(&operation_type_str).unwrap_or(CreditOperationType::SceneProcessing);

    let credits_amount = fields
        .get("credits_amount")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let description = fields
        .get("description")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let balance_after = fields
        .get("balance_after")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let video_id = fields
        .get("video_id")
        .and_then(|v| String::from_firestore_value(v));

    let draft_id = fields
        .get("draft_id")
        .and_then(|v| String::from_firestore_value(v));

    let metadata = fields.get("metadata").and_then(|v| {
        if let Value::MapValue(map) = v {
            map.fields.as_ref().map(|f| {
                f.iter()
                    .filter_map(|(k, v)| {
                        String::from_firestore_value(v).map(|s| (k.clone(), s))
                    })
                    .collect()
            })
        } else {
            None
        }
    });

    let created_at = fields
        .get("created_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or(timestamp);

    Ok(CreditTransaction {
        id: tx_id.to_string(),
        user_id,
        timestamp,
        operation_type,
        credits_amount,
        description,
        balance_after,
        video_id,
        draft_id,
        metadata,
        created_at,
    })
}
