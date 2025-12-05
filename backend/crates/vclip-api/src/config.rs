//! API configuration.

use std::time::Duration;

/// API server configuration.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
    /// CORS origins
    pub cors_origins: Vec<String>,
    /// Rate limit requests per second
    pub rate_limit_rps: u32,
    /// Rate limit burst
    pub rate_limit_burst: u32,
    /// Request timeout
    pub request_timeout: Duration,
    /// Max request body size
    pub max_body_size: usize,
    /// Environment (development/production)
    pub environment: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8000,
            cors_origins: vec!["*".to_string()],
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            request_timeout: Duration::from_secs(30),
            max_body_size: 10 * 1024 * 1024, // 10MB
            environment: "development".to_string(),
        }
    }
}

impl ApiConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("API_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8000),
            cors_origins: std::env::var("CORS_ORIGINS")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_else(|_| vec!["*".to_string()]),
            rate_limit_rps: std::env::var("RATE_LIMIT_RPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20),
            request_timeout: Duration::from_secs(
                std::env::var("REQUEST_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            max_body_size: std::env::var("MAX_BODY_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10 * 1024 * 1024),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
        }
    }

    /// Check if running in production mode.
    pub fn is_production(&self) -> bool {
        self.environment.to_lowercase() == "production"
    }
}
