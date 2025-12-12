//! Secure video delivery layer.
//!
//! This module provides URL generation for clip playback, download, and sharing.
//! It abstracts over presigned URLs and (future) Worker-fronted delivery.

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::client::R2Client;
use crate::error::{StorageError, StorageResult};

// ============================================================================
// Configuration
// ============================================================================

/// Default expiry for playback URLs (15 minutes).
pub const DEFAULT_PLAYBACK_EXPIRY_SECS: u64 = 900;

/// Default expiry for download URLs (5 minutes).
pub const DEFAULT_DOWNLOAD_EXPIRY_SECS: u64 = 300;

/// Default expiry for share URLs (24 hours - the redirect generates fresh short-lived URLs).
pub const DEFAULT_SHARE_REDIRECT_EXPIRY_SECS: u64 = 86400;

/// Maximum allowed expiry (7 days) to prevent long-lived URL leakage.
pub const MAX_EXPIRY_SECS: u64 = 604800;

/// Delivery configuration.
#[derive(Debug, Clone)]
pub struct DeliveryConfig {
    /// Secret key for HMAC signing (for Worker tokens, future use).
    pub signing_secret: Option<String>,
    /// Base URL for Worker-fronted delivery (e.g., https://cdn.viralclipai.io).
    pub worker_base_url: Option<String>,
    /// Whether to prefer Worker URLs when available.
    pub prefer_worker: bool,
    /// Default playback URL expiry.
    pub playback_expiry: Duration,
    /// Default download URL expiry.
    pub download_expiry: Duration,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            signing_secret: None,
            worker_base_url: None,
            prefer_worker: false,
            playback_expiry: Duration::from_secs(DEFAULT_PLAYBACK_EXPIRY_SECS),
            download_expiry: Duration::from_secs(DEFAULT_DOWNLOAD_EXPIRY_SECS),
        }
    }
}

impl DeliveryConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        let signing_secret = std::env::var("DELIVERY_SIGNING_SECRET").ok();
        let worker_base_url = std::env::var("CDN_WORKER_URL").ok();
        
        // Default to using worker when both worker URL and signing secret are configured.
        // This handles the case where R2 public access is disabled.
        // Can be explicitly disabled with PREFER_WORKER_DELIVERY=false.
        let prefer_worker = std::env::var("PREFER_WORKER_DELIVERY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or_else(|_| {
                // Default to true if worker is fully configured
                worker_base_url.is_some() && signing_secret.is_some()
            });
        
        Self {
            signing_secret,
            worker_base_url,
            prefer_worker,
            playback_expiry: Duration::from_secs(
                std::env::var("PLAYBACK_URL_EXPIRY_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_PLAYBACK_EXPIRY_SECS)
                    .min(MAX_EXPIRY_SECS),
            ),
            download_expiry: Duration::from_secs(
                std::env::var("DOWNLOAD_URL_EXPIRY_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_DOWNLOAD_EXPIRY_SECS)
                    .min(MAX_EXPIRY_SECS),
            ),
        }
    }

    /// Check if Worker delivery is available and preferred.
    pub fn should_use_worker(&self) -> bool {
        self.prefer_worker && self.worker_base_url.is_some() && self.signing_secret.is_some()
    }
}

// ============================================================================
// Delivery URL Types
// ============================================================================

/// Scope of the delivery URL (what operations are allowed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryScope {
    /// Playback only (streaming in video player).
    Playback,
    /// Download with Content-Disposition: attachment.
    Download,
    /// Thumbnail access.
    Thumbnail,
}

impl DeliveryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeliveryScope::Playback => "play",
            DeliveryScope::Download => "dl",
            DeliveryScope::Thumbnail => "thumb",
        }
    }
}

/// Token payload for Worker-fronted delivery (HMAC-signed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryToken {
    /// Clip ID.
    pub cid: String,
    /// User ID (owner).
    pub uid: String,
    /// Scope (play/dl/thumb).
    pub scope: String,
    /// Expiry timestamp (Unix seconds).
    pub exp: u64,
    /// R2 object key - enables stateless Worker delivery.
    /// The Worker trusts this key because the token is HMAC-signed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r2_key: Option<String>,
    /// Optional: is this a public share access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<bool>,
    /// Optional: watermark flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wm: Option<bool>,
}

impl DeliveryToken {
    /// Create a new delivery token.
    pub fn new(clip_id: &str, user_id: &str, scope: DeliveryScope, expiry: Duration) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            cid: clip_id.to_string(),
            uid: user_id.to_string(),
            scope: scope.as_str().to_string(),
            exp: now + expiry.as_secs(),
            r2_key: None,
            share: None,
            wm: None,
        }
    }

    /// Create a new delivery token with an R2 key for stateless Worker delivery.
    pub fn with_r2_key(clip_id: &str, user_id: &str, r2_key: &str, scope: DeliveryScope, expiry: Duration) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            cid: clip_id.to_string(),
            uid: user_id.to_string(),
            scope: scope.as_str().to_string(),
            exp: now + expiry.as_secs(),
            r2_key: Some(r2_key.to_string()),
            share: None,
            wm: None,
        }
    }

    /// Mark as share access.
    pub fn with_share(mut self) -> Self {
        self.share = Some(true);
        self
    }

    /// Mark for watermarking.
    pub fn with_watermark(mut self) -> Self {
        self.wm = Some(true);
        self
    }

    /// Check if token is expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.exp
    }

    /// Encode token to base64 JSON.
    ///
    /// Returns `None` if serialization fails (should not happen with valid tokens).
    pub fn encode(&self) -> Option<String> {
        let json = serde_json::to_vec(self).ok()?;
        Some(URL_SAFE_NO_PAD.encode(json))
    }

    /// Encode token to base64 JSON, returning an error on failure.
    pub fn try_encode(&self) -> StorageResult<String> {
        let json = serde_json::to_vec(self).map_err(|e| {
            StorageError::ConfigError(format!("Failed to serialize delivery token: {}", e))
        })?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }

    /// Decode token from base64 JSON.
    pub fn decode(encoded: &str) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Sign the token with HMAC-SHA256.
    ///
    /// Returns an error if token encoding or HMAC key creation fails.
    pub fn sign(&self, secret: &str) -> StorageResult<String> {
        type HmacSha256 = Hmac<Sha256>;

        let payload = self.try_encode()?;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| {
            StorageError::ConfigError(format!("Invalid HMAC key: {}", e))
        })?;
        mac.update(payload.as_bytes());
        let signature = mac.finalize().into_bytes();

        Ok(format!("{}.{}", payload, URL_SAFE_NO_PAD.encode(signature)))
    }

    /// Verify a signed token.
    ///
    /// Returns `None` if the token is invalid, expired, or signature verification fails.
    /// Returns an error only for configuration issues (invalid HMAC key).
    pub fn verify(signed: &str, secret: &str) -> StorageResult<Option<Self>> {
        type HmacSha256 = Hmac<Sha256>;

        let parts: Vec<&str> = signed.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Ok(None);
        }

        let (payload, sig_encoded) = (parts[0], parts[1]);
        let sig_bytes = match URL_SAFE_NO_PAD.decode(sig_encoded) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| {
            StorageError::ConfigError(format!("Invalid HMAC key: {}", e))
        })?;
        mac.update(payload.as_bytes());

        if mac.verify_slice(&sig_bytes).is_err() {
            return Ok(None);
        }

        let token = match Self::decode(payload) {
            Some(t) => t,
            None => return Ok(None),
        };

        if token.is_expired() {
            return Ok(None);
        }

        Ok(Some(token))
    }
}

// ============================================================================
// Delivery Response
// ============================================================================

/// Response containing a delivery URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryUrl {
    /// The URL to use for playback/download.
    pub url: String,
    /// When this URL expires (ISO 8601).
    pub expires_at: String,
    /// Expiry in seconds from now.
    pub expires_in_secs: u64,
    /// Content type hint.
    pub content_type: String,
}

// ============================================================================
// Delivery URL Generator
// ============================================================================

/// URL generator for clip delivery.
pub struct DeliveryUrlGenerator {
    r2_client: R2Client,
    config: DeliveryConfig,
}

impl DeliveryUrlGenerator {
    /// Create a new generator.
    pub fn new(r2_client: R2Client, config: DeliveryConfig) -> Self {
        Self { r2_client, config }
    }

    /// Generate a playback URL for a clip.
    ///
    /// # Arguments
    /// * `r2_key` - The R2 object key for the clip video.
    /// * `clip_id` - The clip ID (for Worker tokens).
    /// * `user_id` - The user ID (owner).
    pub async fn playback_url(
        &self,
        r2_key: &str,
        clip_id: &str,
        user_id: &str,
    ) -> StorageResult<DeliveryUrl> {
        self.generate_url(r2_key, clip_id, user_id, DeliveryScope::Playback, None)
            .await
    }

    /// Generate a download URL for a clip.
    ///
    /// # Arguments
    /// * `r2_key` - The R2 object key for the clip video.
    /// * `clip_id` - The clip ID.
    /// * `user_id` - The user ID (owner).
    /// * `filename` - Suggested download filename.
    pub async fn download_url(
        &self,
        r2_key: &str,
        clip_id: &str,
        user_id: &str,
        filename: Option<&str>,
    ) -> StorageResult<DeliveryUrl> {
        self.generate_url(r2_key, clip_id, user_id, DeliveryScope::Download, filename)
            .await
    }

    /// Generate a thumbnail URL.
    pub async fn thumbnail_url(
        &self,
        r2_key: &str,
        clip_id: &str,
        user_id: &str,
    ) -> StorageResult<DeliveryUrl> {
        self.generate_url(r2_key, clip_id, user_id, DeliveryScope::Thumbnail, None)
            .await
    }

    /// Generate a URL for Worker-fronted public share access.
    ///
    /// This creates a signed token that the Worker validates.
    /// The r2_key is embedded in the token for stateless Worker delivery.
    pub fn share_url(
        &self,
        clip_id: &str,
        user_id: &str,
        r2_key: &str,
        watermark: bool,
    ) -> StorageResult<DeliveryUrl> {
        let secret = self.config.signing_secret.as_ref().ok_or_else(|| {
            StorageError::ConfigError("DELIVERY_SIGNING_SECRET not configured".to_string())
        })?;

        let base_url = self.config.worker_base_url.as_ref().ok_or_else(|| {
            StorageError::ConfigError("CDN_WORKER_URL not configured".to_string())
        })?;

        let expiry = Duration::from_secs(DEFAULT_SHARE_REDIRECT_EXPIRY_SECS);
        let mut token = DeliveryToken::with_r2_key(clip_id, user_id, r2_key, DeliveryScope::Playback, expiry)
            .with_share();

        if watermark {
            token = token.with_watermark();
        }

        let signed = token.sign(secret)?;
        let url = format!("{}/v/{}?sig={}", base_url.trim_end_matches('/'), clip_id, signed);

        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expiry.as_secs() as i64);

        Ok(DeliveryUrl {
            url,
            expires_at: expires_at.to_rfc3339(),
            expires_in_secs: expiry.as_secs(),
            content_type: "video/mp4".to_string(),
        })
    }

    /// Internal URL generation.
    async fn generate_url(
        &self,
        r2_key: &str,
        clip_id: &str,
        user_id: &str,
        scope: DeliveryScope,
        filename: Option<&str>,
    ) -> StorageResult<DeliveryUrl> {
        // Decide expiry based on scope
        let expiry = match scope {
            DeliveryScope::Playback | DeliveryScope::Thumbnail => self.config.playback_expiry,
            DeliveryScope::Download => self.config.download_expiry,
        };

        // If Worker delivery is configured and preferred, use it with r2_key for stateless delivery
        if self.config.should_use_worker() {
            return self.generate_worker_url_with_key(clip_id, user_id, Some(r2_key), scope, expiry);
        }

        // Otherwise, use presigned URLs directly to R2
        self.generate_presigned_url(r2_key, scope, expiry, filename)
            .await
    }

    /// Generate presigned URL directly to R2.
    async fn generate_presigned_url(
        &self,
        r2_key: &str,
        scope: DeliveryScope,
        expiry: Duration,
        filename: Option<&str>,
    ) -> StorageResult<DeliveryUrl> {
        // For download scope, we'd ideally set Content-Disposition, but presigned URLs
        // don't easily support response headers without query params.
        // The client can handle this via download attribute on anchor tags.
        let url = self.r2_client.presign_get(r2_key, expiry).await?;

        let content_type = match scope {
            DeliveryScope::Playback | DeliveryScope::Download => "video/mp4",
            DeliveryScope::Thumbnail => "image/jpeg",
        };

        let expires_at = chrono::Utc::now() + chrono::Duration::from_std(expiry).unwrap_or_default();

        // For downloads, append response-content-disposition if the URL supports it
        let final_url = if scope == DeliveryScope::Download {
            if let Some(name) = filename {
                // R2 supports response-content-disposition query param for presigned URLs
                let disposition = format!("attachment; filename=\"{}\"", name);
                let encoded = urlencoding::encode(&disposition);
                if url.contains('?') {
                    format!("{}&response-content-disposition={}", url, encoded)
                } else {
                    format!("{}?response-content-disposition={}", url, encoded)
                }
            } else {
                url
            }
        } else {
            url
        };

        Ok(DeliveryUrl {
            url: final_url,
            expires_at: expires_at.to_rfc3339(),
            expires_in_secs: expiry.as_secs(),
            content_type: content_type.to_string(),
        })
    }

    /// Generate Worker-fronted URL with signed token including R2 key.
    /// This enables fully stateless Worker delivery.
    pub fn generate_worker_url_with_key(
        &self,
        clip_id: &str,
        user_id: &str,
        r2_key: Option<&str>,
        scope: DeliveryScope,
        expiry: Duration,
    ) -> StorageResult<DeliveryUrl> {
        let secret = self.config.signing_secret.as_ref().ok_or_else(|| {
            StorageError::ConfigError("DELIVERY_SIGNING_SECRET not configured".to_string())
        })?;

        let base_url = self.config.worker_base_url.as_ref().ok_or_else(|| {
            StorageError::ConfigError("CDN_WORKER_URL not configured".to_string())
        })?;

        // Create token with or without R2 key
        let token = if let Some(key) = r2_key {
            DeliveryToken::with_r2_key(clip_id, user_id, key, scope, expiry)
        } else {
            DeliveryToken::new(clip_id, user_id, scope, expiry)
        };
        let signed = token.sign(secret)?;

        let path = match scope {
            DeliveryScope::Playback | DeliveryScope::Download => format!("/v/{}", clip_id),
            DeliveryScope::Thumbnail => format!("/t/{}", clip_id),
        };

        let url = format!("{}{}?sig={}", base_url.trim_end_matches('/'), path, signed);

        let content_type = match scope {
            DeliveryScope::Playback | DeliveryScope::Download => "video/mp4",
            DeliveryScope::Thumbnail => "image/jpeg",
        };

        let expires_at = chrono::Utc::now() + chrono::Duration::from_std(expiry).unwrap_or_default();

        Ok(DeliveryUrl {
            url,
            expires_at: expires_at.to_rfc3339(),
            expires_in_secs: expiry.as_secs(),
            content_type: content_type.to_string(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_token_encode_decode() {
        let token = DeliveryToken::new("clip-123", "user-456", DeliveryScope::Playback, Duration::from_secs(3600));
        let encoded = token.encode().expect("should encode");
        let decoded = DeliveryToken::decode(&encoded).expect("should decode");

        assert_eq!(decoded.cid, "clip-123");
        assert_eq!(decoded.uid, "user-456");
        assert_eq!(decoded.scope, "play");
    }

    #[test]
    fn test_delivery_token_sign_verify() {
        let secret = "test-secret-key-32-bytes-long!!!";
        let token = DeliveryToken::new("clip-123", "user-456", DeliveryScope::Playback, Duration::from_secs(3600));
        let signed = token.sign(secret).expect("should sign");

        let verified = DeliveryToken::verify(&signed, secret)
            .expect("should not error")
            .expect("should verify");
        assert_eq!(verified.cid, "clip-123");
    }

    #[test]
    fn test_delivery_token_wrong_secret() {
        let secret = "test-secret-key-32-bytes-long!!!";
        let token = DeliveryToken::new("clip-123", "user-456", DeliveryScope::Playback, Duration::from_secs(3600));
        let signed = token.sign(secret).expect("should sign");

        let result = DeliveryToken::verify(&signed, "wrong-secret").expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_delivery_token_expired() {
        let secret = "test-secret-key-32-bytes-long!!!";
        // Create token that expired 1 second ago
        let mut token = DeliveryToken::new("clip-123", "user-456", DeliveryScope::Playback, Duration::from_secs(0));
        token.exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() - 1;
        let signed = token.sign(secret).expect("should sign");

        let result = DeliveryToken::verify(&signed, secret).expect("should not error");
        assert!(result.is_none(), "expired token should not verify");
    }

    #[test]
    fn test_delivery_scope_str() {
        assert_eq!(DeliveryScope::Playback.as_str(), "play");
        assert_eq!(DeliveryScope::Download.as_str(), "dl");
        assert_eq!(DeliveryScope::Thumbnail.as_str(), "thumb");
    }
}
