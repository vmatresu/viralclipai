//! Token caching for Firestore authentication.
//!
//! Provides a thread-safe, async-aware token cache with:
//! - Refresh margin to avoid token expiry during requests
//! - Single-flight pattern to prevent thundering herd on refresh
//! - Graceful fallback to existing valid token on refresh failure

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use gcp_auth::TokenProvider;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::{FirestoreError, FirestoreResult};

// =============================================================================
// Constants
// =============================================================================

/// Refresh margin: refresh token 60 seconds before expiry.
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// Conservative token TTL when expiry is unknown (50 minutes).
/// OAuth tokens are typically valid for 60 minutes.
const TOKEN_DEFAULT_TTL: Duration = Duration::from_secs(50 * 60);

/// OAuth scope for Firestore/Datastore access.
/// Uses datastore scope which provides necessary permissions for Firestore REST API.
pub const FIRESTORE_SCOPE: &str = "https://www.googleapis.com/auth/datastore";

// =============================================================================
// Token Cache
// =============================================================================

/// Cached token with expiration tracking.
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

impl CachedToken {
    /// Check if token is still valid with refresh margin.
    fn is_valid(&self) -> bool {
        Instant::now() + TOKEN_REFRESH_MARGIN < self.expires_at
    }

    /// Check if token is technically still usable (even if refresh is needed).
    fn is_usable(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// Thread-safe token cache with single-flight refresh.
pub struct TokenCache {
    auth: Arc<dyn TokenProvider>,
    cache: RwLock<Option<CachedToken>>,
}

impl TokenCache {
    /// Create a new token cache.
    pub fn new(auth: Arc<dyn TokenProvider>) -> Self {
        Self {
            auth,
            cache: RwLock::new(None),
        }
    }

    /// Invalidate the cached token.
    pub async fn invalidate(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }

    /// Get a valid access token, refreshing if necessary.
    ///
    /// This method implements the single-flight pattern:
    /// - Fast path: return cached token if still valid
    /// - Slow path: acquire write lock and refresh (double-check first)
    /// - Fallback: on refresh failure, use existing token if still usable
    pub async fn get_token(&self) -> FirestoreResult<String> {
        // Fast path: check read lock first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.is_valid() {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        // Slow path: acquire write lock and refresh
        let mut cache = self.cache.write().await;

        // Double-check: another task may have refreshed while we waited
        if let Some(cached) = cache.as_ref() {
            if cached.is_valid() {
                return Ok(cached.access_token.clone());
            }
        }

        // Attempt refresh
        self.refresh_token(&mut cache).await
    }

    /// Refresh the token, updating the cache.
    async fn refresh_token(
        &self,
        cache: &mut Option<CachedToken>,
    ) -> FirestoreResult<String> {
        let refresh_result = self.auth.token(&[FIRESTORE_SCOPE]).await;

        match refresh_result {
            Ok(token) => {
                let access_token = token.as_str().to_string();

                // Prefer the real expiry from gcp_auth, fall back to a conservative default.
                let expires_at = {
                    let now = Utc::now();
                    let exp = token.expires_at();

                    if exp > now {
                        match (exp - now).to_std() {
                            Ok(ttl) => Instant::now() + ttl,
                            Err(_) => Instant::now() + TOKEN_DEFAULT_TTL,
                        }
                    } else {
                        // Treat already-expired tokens as having a near-immediate expiry so we
                        // force refresh on the next request.
                        Instant::now()
                    }
                };

                *cache = Some(CachedToken {
                    access_token: access_token.clone(),
                    expires_at,
                });

                debug!("Refreshed Firestore auth token, valid for ~50 minutes");
                Ok(access_token)
            }
            Err(e) => {
                // On refresh failure, check if existing token is still usable
                if let Some(cached) = cache.as_ref() {
                    if cached.is_usable() {
                        warn!(
                            "Token refresh failed, using existing token: {}",
                            e
                        );
                        return Ok(cached.access_token.clone());
                    }
                }

                // No valid token available
                Err(FirestoreError::auth_error(format!(
                    "Failed to obtain auth token: {}",
                    e
                )))
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_refresh_margin() {
        assert_eq!(TOKEN_REFRESH_MARGIN, Duration::from_secs(60));
    }

    #[test]
    fn test_token_default_ttl() {
        assert_eq!(TOKEN_DEFAULT_TTL, Duration::from_secs(50 * 60));
    }

    #[test]
    fn test_firestore_scope() {
        assert!(FIRESTORE_SCOPE.contains("datastore"));
    }
}
