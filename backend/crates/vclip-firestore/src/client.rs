//! Firestore REST API client.
//!
//! Production-grade client with:
//! - Token caching with refresh margin
//! - HTTP client tuning (pooling, timeouts)
//! - Exponential backoff with jitter
//! - Observability (tracing spans, metrics)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gcp_auth::{CustomServiceAccount, TokenProvider};
use reqwest::{Client, StatusCode};
use tracing::{debug, info_span, Instrument};

use crate::error::{FirestoreError, FirestoreResult};
use crate::metrics::{names as metric_names, record_request};
use crate::retry::RetryConfig;
use crate::token_cache::TokenCache;
use crate::types::{
    BatchGetDocumentsRequest, BatchGetDocumentsResponse, BatchWriteRequest, BatchWriteResponse,
    Document, DocumentMask, ListDocumentsResponse, RunQueryRequest, RunQueryResponse,
    StructuredQuery, Value, Write,
};

// =============================================================================
// Configuration
// =============================================================================

/// Firestore client configuration.
#[derive(Debug, Clone)]
pub struct FirestoreConfig {
    /// GCP project ID
    pub project_id: String,
    /// Database ID (usually "(default)")
    pub database_id: String,
    /// Request timeout
    pub timeout: Duration,
    /// Connect timeout
    pub connect_timeout: Duration,
    /// Retry configuration
    pub retry: RetryConfig,
}

impl FirestoreConfig {
    /// Create config from environment variables.
    pub fn from_env() -> FirestoreResult<Self> {
        let project_id = std::env::var("GCP_PROJECT_ID")
            .or_else(|_| std::env::var("FIREBASE_PROJECT_ID"))
            .map_err(|_| {
                FirestoreError::auth_error(
                    "GCP_PROJECT_ID or FIREBASE_PROJECT_ID must be set to access Firestore",
                )
            })?;

        if project_id.is_empty() {
            return Err(FirestoreError::auth_error(
                "GCP_PROJECT_ID or FIREBASE_PROJECT_ID cannot be empty",
            ));
        }

        let connect_timeout_secs: u64 = std::env::var("FIRESTORE_CONNECT_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        Ok(Self {
            project_id,
            database_id: std::env::var("FIRESTORE_DATABASE_ID")
                .unwrap_or_else(|_| "(default)".to_string()),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(connect_timeout_secs),
            retry: RetryConfig::from_env(),
        })
    }
}

// =============================================================================
// Client
// =============================================================================

/// Firestore REST API client.
pub struct FirestoreClient {
    http: Client,
    config: FirestoreConfig,
    base_url: String,
    token_cache: Arc<TokenCache>,
}

impl Clone for FirestoreClient {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            config: self.config.clone(),
            base_url: self.base_url.clone(),
            token_cache: Arc::clone(&self.token_cache),
        }
    }
}

impl FirestoreClient {
    /// Create a new Firestore client.
    pub async fn new(config: FirestoreConfig) -> FirestoreResult<Self> {
        let auth = Self::create_auth_provider()?;

        let http = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .user_agent(concat!("vclip-firestore/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(FirestoreError::Network)?;

        let base_url = format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/{}/documents",
            config.project_id, config.database_id
        );

        Ok(Self {
            http,
            config,
            base_url,
            token_cache: Arc::new(TokenCache::new(auth)),
        })
    }

    fn create_auth_provider() -> FirestoreResult<Arc<dyn TokenProvider>> {
        let service_account = CustomServiceAccount::from_env()
            .map_err(|e| FirestoreError::auth_error(format!("Failed to load service account: {}", e)))?;

        match service_account {
            Some(sa) => Ok(Arc::new(sa)),
            None => Err(FirestoreError::auth_error(
                "GOOGLE_APPLICATION_CREDENTIALS not set. \
                 Set it to the path of your service account JSON file.",
            )),
        }
    }

    /// Create from environment variables.
    pub async fn from_env() -> FirestoreResult<Self> {
        let config = FirestoreConfig::from_env()?;
        Self::new(config).await
    }

    /// Get an access token.
    async fn get_token(&self) -> FirestoreResult<String> {
        self.token_cache.get_token().await
    }

    fn is_access_token_expired(body: &str) -> bool {
        body.contains("ACCESS_TOKEN_EXPIRED") || body.contains("\"UNAUTHENTICATED\"")
    }

    /// Build document path.
    fn document_path(&self, collection: &str, doc_id: &str) -> String {
        format!("{}/{}/{}", self.base_url, collection, doc_id)
    }

    // =========================================================================
    // CRUD Operations
    // =========================================================================

    /// Get a document.
    pub async fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> FirestoreResult<Option<Document>> {
        let url = self.document_path(collection, doc_id);

        self.execute_request("get_document", collection, Some(doc_id), async {
            let mut token = self.get_token().await?;
            let mut response = self.http.get(&url).bearer_auth(&token).send().await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self.http.get(&url).bearer_auth(&token).send().await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let doc: Document = response.json().await?;
                    Ok(Some(doc))
                }
                StatusCode::NOT_FOUND => Ok(None),
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    /// Create a document.
    pub async fn create_document(
        &self,
        collection: &str,
        doc_id: &str,
        fields: HashMap<String, Value>,
    ) -> FirestoreResult<Document> {
        let url = format!("{}/{}?documentId={}", self.base_url, collection, doc_id);
        let body = Document::new(fields);

        self.execute_request("create_document", collection, Some(doc_id), async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .post(&url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body_text = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body_text) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .post(&url)
                        .bearer_auth(&token)
                        .json(&body)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body_text),
                    ));
                }
            }

            match status {
                StatusCode::OK | StatusCode::CREATED => {
                    let doc: Document = response.json().await?;
                    Ok(doc)
                }
                StatusCode::CONFLICT => Err(FirestoreError::AlreadyExists(format!(
                    "{}/{}",
                    collection, doc_id
                ))),
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
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
        if let Some(mask) = update_mask {
            let params: Vec<String> = mask.iter().map(|f| format!("updateMask.fieldPaths={}", f)).collect();
            url = format!("{}?{}", url, params.join("&"));
        }

        let body = Document::new(fields);

        self.execute_request("update_document", collection, Some(doc_id), async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .patch(&url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body_text = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body_text) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .patch(&url)
                        .bearer_auth(&token)
                        .json(&body)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body_text),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let doc: Document = response.json().await?;
                    Ok(doc)
                }
                StatusCode::NOT_FOUND => Err(FirestoreError::not_found(format!("{}/{}", collection, doc_id))),
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    /// Update with optimistic concurrency control.
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
            params.extend(mask.iter().map(|f| format!("updateMask.fieldPaths={}", f)));
        }
        if let Some(ts) = update_time {
            params.push(format!("currentDocument.updateTime={}", urlencoding::encode(ts)));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let body = Document::new(fields);

        self.execute_request("update_document_precondition", collection, Some(doc_id), async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .patch(&url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body_text = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body_text) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .patch(&url)
                        .bearer_auth(&token)
                        .json(&body)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body_text),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let doc: Document = response.json().await?;
                    Ok(doc)
                }
                StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => {
                    let body_text = response.text().await.unwrap_or_default();
                    Err(FirestoreError::PreconditionFailed(format!(
                        "Precondition failed: {}",
                        body_text
                    )))
                }
                StatusCode::NOT_FOUND => Err(FirestoreError::not_found(format!("{}/{}", collection, doc_id))),
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    /// Delete a document.
    pub async fn delete_document(&self, collection: &str, doc_id: &str) -> FirestoreResult<()> {
        let url = self.document_path(collection, doc_id);
        let coll = collection.to_string();
        let id = doc_id.to_string();

        self.execute_request("delete_document", collection, Some(doc_id), async {
            let mut token = self.get_token().await?;
            let mut response = self.http.delete(&url).bearer_auth(&token).send().await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self.http.delete(&url).bearer_auth(&token).send().await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
                StatusCode::NOT_FOUND => {
                    debug!("Document {}/{} already deleted (idempotent)", coll, id);
                    Ok(())
                }
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
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

        self.execute_request("list_documents", collection, None, async {
            let mut token = self.get_token().await?;
            let mut response = self.http.get(&url).bearer_auth(&token).send().await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self.http.get(&url).bearer_auth(&token).send().await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let list: ListDocumentsResponse = response.json().await?;
                    let returned = list.documents.as_ref().map(|d| d.len()).unwrap_or(0) as u64;
                    metrics::counter!(
                        metric_names::LIST_DOCUMENTS_RETURNED_TOTAL,
                        "collection" => collection.to_string()
                    )
                    .increment(returned);
                    Ok(list)
                }
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    /// Batch get multiple documents using Firestore documents:batchGet.
    ///
    /// Returns a vector of Documents in arbitrary order (matching Firestore response ordering).
    /// Missing documents are omitted.
    pub async fn batch_get_documents(
        &self,
        full_document_names: Vec<String>,
        mask: Option<DocumentMask>,
    ) -> FirestoreResult<Vec<Document>> {
        if full_document_names.is_empty() {
            return Ok(vec![]);
        }
        if full_document_names.len() > 100 {
            return Err(FirestoreError::request_failed(
                "Batch get exceeds 100 document limit".to_string(),
            ));
        }

        let url = format!("{}:batchGet", self.base_url);
        let request = BatchGetDocumentsRequest {
            documents: full_document_names,
            mask,
        };

        self.execute_request("batch_get_documents", "batch", None, async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .post(&url)
                .bearer_auth(&token)
                .json(&request)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .post(&url)
                        .bearer_auth(&token)
                        .json(&request)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let body = response.text().await.unwrap_or_default();
                    // Firestore batchGet returns a JSON array of BatchGetDocumentsResponse objects
                    let responses: Vec<BatchGetDocumentsResponse> =
                        serde_json::from_str(&body).map_err(|e| {
                            FirestoreError::request_failed(format!(
                                "Failed to parse batchGet response: {} (body prefix: {})",
                                e,
                                &body[..body.len().min(200)]
                            ))
                        })?;

                    let docs: Vec<Document> = responses
                        .into_iter()
                        .filter_map(|r| r.found)
                        .collect();

                    Ok(docs)
                }
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    // =========================================================================
    // Batch Operations
    // =========================================================================

    /// Build full document name for batch operations.
    pub fn full_document_name(&self, collection: &str, doc_id: &str) -> String {
        format!(
            "projects/{}/databases/{}/documents/{}/{}",
            self.config.project_id, self.config.database_id, collection, doc_id
        )
    }

    /// Execute a batch write (atomic multi-document operation).
    pub async fn batch_write(&self, writes: Vec<Write>) -> FirestoreResult<BatchWriteResponse> {
        if writes.is_empty() {
            return Ok(BatchWriteResponse::empty());
        }
        if writes.len() > 500 {
            return Err(FirestoreError::request_failed("Batch write exceeds 500 document limit"));
        }

        let url = format!("{}:batchWrite", self.base_url);
        let request = BatchWriteRequest { writes };

        self.execute_request("batch_write", "batch", None, async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .post(&url)
                .bearer_auth(&token)
                .json(&request)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .post(&url)
                        .bearer_auth(&token)
                        .json(&request)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let batch_response: BatchWriteResponse = response.json().await?;
                    batch_response.check_for_errors()?;
                    Ok(batch_response)
                }
                StatusCode::CONFLICT => Err(FirestoreError::AlreadyExists("Batch write conflict".to_string())),
                StatusCode::PRECONDITION_FAILED => Err(FirestoreError::PreconditionFailed("Batch precondition failed".to_string())),
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    /// Execute with retry.
    pub async fn with_retry<T, F, Fut>(&self, operation: &str, op: F) -> FirestoreResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = FirestoreResult<T>>,
    {
        crate::retry::with_retry(&self.config.retry, operation, op).await
    }

    // =========================================================================
    // Query Operations
    // =========================================================================

    /// Run a structured query on a collection.
    ///
    /// The `parent_path` should be the path containing the collection, e.g.,
    /// "users/USER_ID" for querying "users/USER_ID/credit_transactions".
    pub async fn run_query(
        &self,
        parent_path: &str,
        query: StructuredQuery,
    ) -> FirestoreResult<Vec<Document>> {
        let url = format!("{}/{}:runQuery", self.base_url, parent_path);
        let request = RunQueryRequest {
            structured_query: query,
        };

        self.execute_request("run_query", parent_path, None, async {
            let mut token = self.get_token().await?;
            let mut response = self
                .http
                .post(&url)
                .bearer_auth(&token)
                .json(&request)
                .send()
                .await?;
            let mut status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                let body = response.text().await.unwrap_or_default();
                if Self::is_access_token_expired(&body) {
                    self.token_cache.invalidate().await;
                    token = self.get_token().await?;
                    response = self
                        .http
                        .post(&url)
                        .bearer_auth(&token)
                        .json(&request)
                        .send()
                        .await?;
                    status = response.status();
                } else {
                    return Err(FirestoreError::from_http_status(
                        status.as_u16(),
                        format!("{} failed: {}", url, body),
                    ));
                }
            }

            match status {
                StatusCode::OK => {
                    let body = response.text().await.unwrap_or_default();
                    // runQuery returns a JSON array of RunQueryResponse objects
                    let responses: Vec<RunQueryResponse> =
                        serde_json::from_str(&body).map_err(|e| {
                            FirestoreError::request_failed(format!(
                                "Failed to parse runQuery response: {} (body prefix: {})",
                                e,
                                &body[..body.len().min(200)]
                            ))
                        })?;

                    let docs: Vec<Document> = responses
                        .into_iter()
                        .filter_map(|r| r.document)
                        .collect();

                    Ok(docs)
                }
                _ => Err(Self::handle_error_response(status, &url, response).await),
            }
        })
        .await
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Execute a request with tracing and metrics.
    async fn execute_request<T, F>(
        &self,
        operation: &str,
        collection: &str,
        doc_id: Option<&str>,
        fut: F,
    ) -> FirestoreResult<T>
    where
        F: std::future::Future<Output = FirestoreResult<T>>,
    {
        let span = if let Some(id) = doc_id {
            info_span!("firestore_request", operation = %operation, collection = %collection, doc_id = %id)
        } else {
            info_span!("firestore_request", operation = %operation, collection = %collection)
        };

        let start = Instant::now();
        let result = fut.instrument(span).await;
        let latency_ms = start.elapsed().as_millis() as f64;

        let status = match &result {
            Ok(_) => 200,
            Err(e) => e.http_status().unwrap_or(500),
        };
        record_request(operation, status, latency_ms);

        result
    }

    async fn handle_error_response(status: StatusCode, url: &str, response: reqwest::Response) -> FirestoreError {
        let body = response.text().await.unwrap_or_default();
        FirestoreError::from_http_status(status.as_u16(), format!("{} failed: {}", url, body))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_config_from_env_validates_project_id() {
        std::env::remove_var("GCP_PROJECT_ID");
        std::env::remove_var("FIREBASE_PROJECT_ID");
        let result = FirestoreConfig::from_env();
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_config_default_values() {
        std::env::set_var("GCP_PROJECT_ID", "test-project");
        std::env::remove_var("FIRESTORE_CONNECT_TIMEOUT_SECS");
        let config = FirestoreConfig::from_env().unwrap();
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
    }
}
