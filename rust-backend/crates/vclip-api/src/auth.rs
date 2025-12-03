//! Firebase ID token authentication.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::ApiError;
use crate::state::AppState;

/// Google JWKS URL for Firebase Auth.
const GOOGLE_JWKS_URL: &str =
    "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";

/// Firebase token issuer prefix.
const FIREBASE_ISSUER_PREFIX: &str = "https://securetoken.google.com/";

/// JWKS cache TTL.
const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

/// Decoded Firebase ID token claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirebaseClaims {
    /// User ID
    pub sub: String,
    /// Email (if available)
    pub email: Option<String>,
    /// Email verified
    pub email_verified: Option<bool>,
    /// Issuer
    pub iss: String,
    /// Audience (Firebase project ID)
    pub aud: String,
    /// Issued at
    pub iat: i64,
    /// Expiration
    pub exp: i64,
    /// Authentication time
    pub auth_time: Option<i64>,
}

impl FirebaseClaims {
    /// Get user ID (alias for sub).
    pub fn uid(&self) -> &str {
        &self.sub
    }
}

/// Authenticated user extracted from request.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub uid: String,
    pub email: Option<String>,
    pub email_verified: bool,
}

impl From<FirebaseClaims> for AuthUser {
    fn from(claims: FirebaseClaims) -> Self {
        Self {
            uid: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
        }
    }
}

/// JWKS response from Google.
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    kid: String,
    n: String,
    e: String,
}

/// Cached JWKS keys.
pub struct JwksCache {
    http: Client,
    keys: RwLock<HashMap<String, DecodingKey>>,
    last_refresh: RwLock<Instant>,
    project_id: String,
}

impl JwksCache {
    /// Create a new JWKS cache.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let project_id = std::env::var("FIREBASE_PROJECT_ID")
            .or_else(|_| std::env::var("GCP_PROJECT_ID"))?;

        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let cache = Self {
            http,
            keys: RwLock::new(HashMap::new()),
            last_refresh: RwLock::new(Instant::now() - JWKS_CACHE_TTL),
            project_id,
        };

        // Initial key refresh
        cache.refresh_keys().await?;

        Ok(cache)
    }

    /// Refresh JWKS keys from Google.
    async fn refresh_keys(&self) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Refreshing JWKS keys");

        let response = self.http.get(GOOGLE_JWKS_URL).send().await?;
        let jwks: JwksResponse = response.json().await?;

        let mut keys = HashMap::new();
        for jwk in jwks.keys {
            let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)?;
            keys.insert(jwk.kid, key);
        }

        let key_count = keys.len();
        *self.keys.write().await = keys;
        *self.last_refresh.write().await = Instant::now();

        debug!("Refreshed {} JWKS keys", key_count);
        Ok(())
    }

    /// Get decoding key for a key ID.
    async fn get_key(&self, kid: &str) -> Option<DecodingKey> {
        // Check if refresh needed
        let needs_refresh = {
            let last = self.last_refresh.read().await;
            last.elapsed() > JWKS_CACHE_TTL
        };

        if needs_refresh {
            if let Err(e) = self.refresh_keys().await {
                warn!("Failed to refresh JWKS keys: {}", e);
            }
        }

        self.keys.read().await.get(kid).cloned()
    }

    /// Verify a Firebase ID token.
    pub async fn verify_token(&self, token: &str) -> Result<FirebaseClaims, ApiError> {
        // Decode header to get key ID
        let header = decode_header(token)
            .map_err(|e| ApiError::unauthorized(format!("Invalid token header: {}", e)))?;

        let kid = header
            .kid
            .ok_or_else(|| ApiError::unauthorized("Token missing key ID"))?;

        // Get decoding key
        let key = self
            .get_key(&kid)
            .await
            .ok_or_else(|| ApiError::unauthorized("Unknown key ID"))?;

        // Set up validation
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[format!("{}{}", FIREBASE_ISSUER_PREFIX, self.project_id)]);
        validation.set_audience(&[&self.project_id]);

        // Decode and validate token
        let token_data = decode::<FirebaseClaims>(token, &key, &validation)
            .map_err(|e| ApiError::unauthorized(format!("Token validation failed: {}", e)))?;

        Ok(token_data.claims)
    }
}

/// Axum extractor for authenticated user.
#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Get Authorization header
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("Missing Authorization header"))?;

        // Extract Bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::unauthorized("Invalid Authorization header format"))?;

        // Verify token
        let claims = state.jwks.verify_token(token).await?;

        Ok(AuthUser::from(claims))
    }
}
