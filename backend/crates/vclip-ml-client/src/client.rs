//! ML service HTTP client.

use std::time::Duration;

use reqwest::Client;
use tracing::{debug, warn};

use crate::error::{MlError, MlResult};
use crate::types::{CropPlan, CropRequest, CropResponse, HealthResponse};

/// Configuration for ML client.
#[derive(Debug, Clone)]
pub struct MlClientConfig {
    /// Base URL of ML service
    pub base_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Max retries
    pub max_retries: u32,
}

impl Default for MlClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8001".to_string(),
            timeout: Duration::from_secs(300), // 5 minutes for video analysis
            max_retries: 2,
        }
    }
}

impl MlClientConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("ML_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8001".to_string()),
            timeout: Duration::from_secs(
                std::env::var("ML_SERVICE_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            max_retries: std::env::var("ML_SERVICE_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
        }
    }
}

/// Client for Python ML service.
pub struct MlClient {
    http: Client,
    config: MlClientConfig,
}

impl MlClient {
    /// Create a new ML client.
    pub fn new(config: MlClientConfig) -> MlResult<Self> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(MlError::Network)?;

        Ok(Self { http, config })
    }

    /// Create from environment variables.
    pub fn from_env() -> MlResult<Self> {
        Self::new(MlClientConfig::from_env())
    }

    /// Check if ML service is healthy.
    pub async fn health_check(&self) -> MlResult<bool> {
        let url = format!("{}/health", self.config.base_url);

        match self.http.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                let health: HealthResponse = response.json().await?;
                Ok(health.status == "healthy" || health.status == "ok")
            }
            Ok(response) => {
                warn!("ML service health check failed: {}", response.status());
                Ok(false)
            }
            Err(e) => {
                warn!("ML service health check error: {}", e);
                Ok(false)
            }
        }
    }

    /// Analyze video and get crop plan.
    pub async fn analyze(&self, request: &CropRequest) -> MlResult<CropPlan> {
        let response = self.analyze_and_render(request).await?;
        Ok(response.crop_plan)
    }

    /// Analyze video and optionally render output.
    pub async fn analyze_and_render(&self, request: &CropRequest) -> MlResult<CropResponse> {
        let url = format!("{}/analyze", self.config.base_url);

        debug!("Sending crop analysis request to {}", url);

        let response = self.with_retry(|| async {
            self.http
                .post(&url)
                .json(request)
                .send()
                .await
                .map_err(MlError::Network)
        })
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MlError::RequestFailed(format!(
                "ML service returned {}: {}",
                status, body
            )));
        }

        let crop_response: CropResponse = response.json().await?;
        Ok(crop_response)
    }

    /// Execute with retry logic.
    async fn with_retry<F, Fut, T>(&self, operation: F) -> MlResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = MlResult<T>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                    let delay = Duration::from_millis(500 * 2u64.pow(attempt));
                    warn!(
                        "ML request failed (attempt {}), retrying in {:?}: {}",
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

        Err(last_error.unwrap_or(MlError::RequestFailed("Unknown error".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = MlClientConfig::default();
        assert_eq!(config.base_url, "http://localhost:8001");
        assert_eq!(config.timeout, Duration::from_secs(300));
    }
}
