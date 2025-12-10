//! Firestore REST API client.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use gcp_auth::TokenProvider;
use reqwest::{Client, StatusCode};
use tracing::{debug, warn};

use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{BatchWriteRequest, BatchWriteResponse, Document, ListDocumentsResponse, Value, Write};

/// Firestore client configuration.
#[derive(Debug, Clone)]
pub struct FirestoreConfig {
    /// GCP project ID
    pub project_id: String,
    /// Database ID (usually "(default)")
    pub database_id: String,
    /// Request timeout
    pub timeout: Duration,
    /// Max retries for transient errors
    pub max_retries: u32,
}

impl FirestoreConfig {
    /// Create config from environment variables.
    pub fn from_env() -> FirestoreResult<Self> {
        Ok(Self {
            project_id: std::env::var("GCP_PROJECT_ID")
                .or_else(|_| std::env::var("FIREBASE_PROJECT_ID"))
                .map_err(|_| FirestoreError::auth_error("GCP_PROJECT_ID not set"))?,
            database_id: std::env::var("FIRESTORE_DATABASE_ID")
                .unwrap_or_else(|_| "(default)".to_string()),
            timeout: Duration::from_secs(30),
            max_retries: 3,
        })
    }
}

/// Firestore REST API client.
pub struct FirestoreClient {
    http: Client,
    auth: Arc<dyn TokenProvider>,
    config: FirestoreConfig,
    base_url: String,
}

impl Clone for FirestoreClient {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            auth: Arc::clone(&self.auth),
            config: self.config.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

impl FirestoreClient {
    /// Create a new Firestore client.
    pub async fn new(config: FirestoreConfig) -> FirestoreResult<Self> {
        let auth = gcp_auth::provider()
            .await
            .map_err(|e| FirestoreError::auth_error(e.to_string()))?;

        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(FirestoreError::Network)?;

        let base_url = format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/{}/documents",
            config.project_id, config.database_id
        );

        Ok(Self {
            http,
            auth,
            config,
            base_url,
        })
    }

    /// Create from environment variables.
    pub async fn from_env() -> FirestoreResult<Self> {
        let config = FirestoreConfig::from_env()?;
        Self::new(config).await
    }

    /// Get an access token.
    async fn get_token(&self) -> FirestoreResult<String> {
        let token = self.auth
            .token(&["https://www.googleapis.com/auth/datastore"])
            .await
            .map_err(|e| FirestoreError::auth_error(e.to_string()))?;
        Ok(token.as_str().to_string())
    }

    /// Build document path.
    fn document_path(&self, collection: &str, doc_id: &str) -> String {
        format!("{}/{}/{}", self.base_url, collection, doc_id)
    }

    /// Get a document.
    pub async fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> FirestoreResult<Option<Document>> {
        let url = self.document_path(collection, doc_id);
        let token = self.get_token().await?;

        let response = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let doc: Document = response.json().await?;
                Ok(Some(doc))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "GET {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// Create a document.
    pub async fn create_document(
        &self,
        collection: &str,
        doc_id: &str,
        fields: HashMap<String, Value>,
    ) -> FirestoreResult<Document> {
        let url = format!("{}/{}?documentId={}", self.base_url, collection, doc_id);
        let token = self.get_token().await?;

        let body = Document {
            name: None,
            fields: Some(fields),
            create_time: None,
            update_time: None,
        };

        let response = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK | StatusCode::CREATED => {
                let doc: Document = response.json().await?;
                Ok(doc)
            }
            StatusCode::CONFLICT => Err(FirestoreError::AlreadyExists(format!(
                "{}/{}",
                collection, doc_id
            ))),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "POST {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// Update a document (merge).
    pub async fn update_document(
        &self,
        collection: &str,
        doc_id: &str,
        fields: HashMap<String, Value>,
        update_mask: Option<Vec<String>>,
    ) -> FirestoreResult<Document> {
        let mut url = self.document_path(collection, doc_id);

        // Add update mask for merge semantics
        if let Some(mask) = update_mask {
            let mask_params: Vec<String> = mask.iter().map(|f| format!("updateMask.fieldPaths={}", f)).collect();
            url = format!("{}?{}", url, mask_params.join("&"));
        }

        let token = self.get_token().await?;

        let body = Document {
            name: None,
            fields: Some(fields),
            create_time: None,
            update_time: None,
        };

        let response = self
            .http
            .patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let doc: Document = response.json().await?;
                Ok(doc)
            }
            StatusCode::NOT_FOUND => Err(FirestoreError::not_found(format!(
                "{}/{}",
                collection, doc_id
            ))),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "PATCH {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// Update a document with an updateTime precondition to avoid lost updates.
    ///
    /// This is useful for optimistic concurrency control where concurrent writers
    /// may contend on the same document.
    pub async fn update_document_with_precondition(
        &self,
        collection: &str,
        doc_id: &str,
        fields: HashMap<String, Value>,
        update_mask: Option<Vec<String>>,
        update_time: Option<&str>,
    ) -> FirestoreResult<Document> {
        let mut url = self.document_path(collection, doc_id);
        let mut params: Vec<String> = Vec::new();

        if let Some(mask) = update_mask {
            params.extend(
                mask.iter()
                    .map(|f| format!("updateMask.fieldPaths={}", f)),
            );
        }

        if let Some(ts) = update_time {
            params.push(format!(
                "currentDocument.updateTime={}",
                urlencoding::encode(ts)
            ));
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let token = self.get_token().await?;

        let body = Document {
            name: None,
            fields: Some(fields),
            create_time: None,
            update_time: None,
        };

        let response = self
            .http
            .patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let doc: Document = response.json().await?;
                Ok(doc)
            }
            StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::PreconditionFailed(format!(
                    "PATCH {} precondition failed: {}",
                    url, body
                )))
            }
            StatusCode::NOT_FOUND => Err(FirestoreError::not_found(format!(
                "{}/{}",
                collection, doc_id
            ))),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "PATCH {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// Delete a document.
    pub async fn delete_document(&self, collection: &str, doc_id: &str) -> FirestoreResult<()> {
        let url = self.document_path(collection, doc_id);
        let token = self.get_token().await?;

        let response = self.http.delete(&url).bearer_auth(&token).send().await?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            StatusCode::NOT_FOUND => {
                // Idempotent delete
                debug!("Document {}/{} already deleted", collection, doc_id);
                Ok(())
            }
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "DELETE {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// List documents in a collection.
    pub async fn list_documents(
        &self,
        collection: &str,
        page_size: Option<u32>,
        page_token: Option<&str>,
    ) -> FirestoreResult<ListDocumentsResponse> {
        let mut url = format!("{}/{}", self.base_url, collection);

        let mut params = Vec::new();
        if let Some(size) = page_size {
            params.push(format!("pageSize={}", size));
        }
        if let Some(token) = page_token {
            params.push(format!("pageToken={}", token));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let token = self.get_token().await?;

        let response = self.http.get(&url).bearer_auth(&token).send().await?;

        match response.status() {
            StatusCode::OK => {
                let list: ListDocumentsResponse = response.json().await?;
                Ok(list)
            }
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "LIST {} failed with {}: {}",
                    url, status, body
                )))
            }
        }
    }

    /// Execute with retry.
    pub async fn with_retry<T, F, Fut>(&self, operation: F) -> FirestoreResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = FirestoreResult<T>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                    let delay = Duration::from_millis(100 * 2u64.pow(attempt));
                    warn!(
                        "Firestore operation failed (attempt {}), retrying in {:?}: {}",
                        attempt + 1,
                        delay,
                        e
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or_else(|| FirestoreError::request_failed("Unknown error")))
    }

    // ========================================================================
    // Batch Write Operations (Atomic multi-document writes)
    // ========================================================================

    /// Build full document name for batch operations.
    pub fn full_document_name(&self, collection: &str, doc_id: &str) -> String {
        format!(
            "projects/{}/databases/{}/documents/{}/{}",
            self.config.project_id, self.config.database_id, collection, doc_id
        )
    }

    /// Execute a batch write (atomic multi-document operation).
    ///
    /// All writes in the batch either succeed or fail together.
    /// Maximum 500 writes per batch (Firestore limit).
    ///
    /// # Arguments
    /// * `writes` - Vector of Write operations to execute atomically
    ///
    /// # Returns
    /// * `Ok(BatchWriteResponse)` - All writes succeeded
    /// * `Err(FirestoreError)` - One or more writes failed (all rolled back)
    pub async fn batch_write(&self, writes: Vec<Write>) -> FirestoreResult<BatchWriteResponse> {
        if writes.is_empty() {
            return Ok(BatchWriteResponse {
                write_results: Some(vec![]),
                status: Some(vec![]),
            });
        }

        if writes.len() > 500 {
            return Err(FirestoreError::request_failed(
                "Batch write exceeds 500 document limit",
            ));
        }

        let url = format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/{}/documents:batchWrite",
            self.config.project_id, self.config.database_id
        );
        let token = self.get_token().await?;

        let request = BatchWriteRequest { writes };

        let response = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let batch_response: BatchWriteResponse = response.json().await?;

                // Check for partial failures in the response
                if let Some(statuses) = &batch_response.status {
                    for (i, status) in statuses.iter().enumerate() {
                        if let Some(code) = status.code {
                            if code != 0 {
                                let msg = status.message.as_deref().unwrap_or("Unknown error");
                                return Err(FirestoreError::request_failed(format!(
                                    "Batch write failed at index {}: {} (code {})",
                                    i, msg, code
                                )));
                            }
                        }
                    }
                }

                Ok(batch_response)
            }
            StatusCode::CONFLICT => Err(FirestoreError::AlreadyExists(
                "Batch write conflict: document already exists".to_string(),
            )),
            StatusCode::PRECONDITION_FAILED => Err(FirestoreError::PreconditionFailed(
                "Batch write precondition failed".to_string(),
            )),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(FirestoreError::request_failed(format!(
                    "Batch write failed with {}: {}",
                    status, body
                )))
            }
        }
    }
}
