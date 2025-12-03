//! Application state.

use std::sync::Arc;

use vclip_firestore::FirestoreClient;
use vclip_queue::{JobQueue, ProgressChannel};
use vclip_storage::R2Client;

use crate::auth::JwksCache;
use crate::config::ApiConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub config: ApiConfig,
    pub storage: Arc<R2Client>,
    pub firestore: Arc<FirestoreClient>,
    pub queue: Arc<JobQueue>,
    pub progress: Arc<ProgressChannel>,
    pub jwks: Arc<JwksCache>,
}

impl AppState {
    /// Create new application state.
    pub async fn new(config: ApiConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let storage = R2Client::from_env().await?;
        let firestore = FirestoreClient::from_env().await?;
        let queue = JobQueue::from_env()?;

        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let progress = ProgressChannel::new(&redis_url)?;

        let jwks = JwksCache::new().await?;

        Ok(Self {
            config,
            storage: Arc::new(storage),
            firestore: Arc::new(firestore),
            queue: Arc::new(queue),
            progress: Arc::new(progress),
            jwks: Arc::new(jwks),
        })
    }
}
