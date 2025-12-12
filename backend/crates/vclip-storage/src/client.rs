//! R2 client implementation.

use std::path::Path;
use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Builder, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use tracing::{debug, info};

use crate::error::{StorageError, StorageResult};

/// Configuration for R2 client.
#[derive(Debug, Clone)]
pub struct R2Config {
    /// R2 endpoint URL (S3 API endpoint)
    pub endpoint_url: String,
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Bucket name
    pub bucket_name: String,
    /// Region (usually "auto" for R2)
    pub region: String,
}

impl R2Config {
    /// Create config from environment variables.
    pub fn from_env() -> StorageResult<Self> {
        Ok(Self {
            endpoint_url: std::env::var("R2_ENDPOINT_URL")
                .map_err(|_| StorageError::config_error("R2_ENDPOINT_URL not set"))?,
            access_key_id: std::env::var("R2_ACCESS_KEY_ID")
                .map_err(|_| StorageError::config_error("R2_ACCESS_KEY_ID not set"))?,
            secret_access_key: std::env::var("R2_SECRET_ACCESS_KEY")
                .map_err(|_| StorageError::config_error("R2_SECRET_ACCESS_KEY not set"))?,
            bucket_name: std::env::var("R2_BUCKET_NAME")
                .map_err(|_| StorageError::config_error("R2_BUCKET_NAME not set"))?,
            region: std::env::var("R2_REGION").unwrap_or_else(|_| "auto".to_string()),
        })
    }
}

/// Cloudflare R2 storage client.
#[derive(Clone)]
pub struct R2Client {
    client: Client,
    bucket: String,
}

impl R2Client {
    /// Create a new R2 client from configuration.
    pub async fn new(config: R2Config) -> StorageResult<Self> {
        let credentials = Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None,
            None,
            "r2",
        );

        let sdk_config = Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&config.endpoint_url)
            .region(Region::new(config.region))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(sdk_config);

        Ok(Self {
            client,
            bucket: config.bucket_name,
        })
    }

    /// Create from environment variables.
    pub async fn from_env() -> StorageResult<Self> {
        let config = R2Config::from_env()?;
        Self::new(config).await
    }

    /// Upload a file to R2.
    pub async fn upload_file(
        &self,
        path: impl AsRef<Path>,
        key: &str,
        content_type: &str,
    ) -> StorageResult<()> {
        let path = path.as_ref();
        debug!("Uploading {} to {}", path.display(), key);

        let body = ByteStream::from_path(path)
            .await
            .map_err(|e| StorageError::upload_failed(e.to_string()))?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| StorageError::upload_failed(e.to_string()))?;

        info!("Uploaded {} to {}", path.display(), key);
        Ok(())
    }

    /// Upload bytes to R2.
    pub async fn upload_bytes(
        &self,
        data: Vec<u8>,
        key: &str,
        content_type: &str,
    ) -> StorageResult<()> {
        debug!("Uploading {} bytes to {}", data.len(), key);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| StorageError::upload_failed(e.to_string()))?;

        Ok(())
    }

    /// Download object as bytes.
    pub async fn download_bytes(&self, key: &str) -> StorageResult<Vec<u8>> {
        debug!("Downloading {}", key);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("NoSuchKey") {
                    StorageError::not_found(key)
                } else {
                    StorageError::DownloadFailed(e.to_string())
                }
            })?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| StorageError::DownloadFailed(e.to_string()))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    /// Download object to a file.
    pub async fn download_file(&self, key: &str, path: impl AsRef<Path>) -> StorageResult<()> {
        let path = path.as_ref();
        debug!("Downloading {} to {}", key, path.display());

        let bytes = self.download_bytes(key).await?;
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::DownloadFailed(format!("Failed to create directory: {}", e)))?;
        }

        tokio::fs::write(path, bytes)
            .await
            .map_err(|e| StorageError::DownloadFailed(format!("Failed to write file: {}", e)))?;

        info!("Downloaded {} to {}", key, path.display());
        Ok(())
    }

    /// Get object with optional byte range.
    pub async fn get_object_range(
        &self,
        key: &str,
        range: Option<&str>,
    ) -> StorageResult<(Vec<u8>, u64, String)> {
        let mut request = self.client.get_object().bucket(&self.bucket).key(key);

        if let Some(r) = range {
            request = request.range(r);
        }

        let response = request.send().await.map_err(|e| {
            if e.to_string().contains("NoSuchKey") {
                StorageError::not_found(key)
            } else {
                StorageError::DownloadFailed(e.to_string())
            }
        })?;

        let content_length = response.content_length().unwrap_or(0) as u64;
        let content_type = response
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| StorageError::DownloadFailed(e.to_string()))?
            .into_bytes()
            .to_vec();

        Ok((bytes, content_length, content_type))
    }

    /// Generate a presigned URL for GET (temporary, signed URL via S3 API).
    pub async fn presign_get(&self, key: &str, expires_in: Duration) -> StorageResult<String> {
        let presign_config = PresigningConfig::expires_in(expires_in)
            .map_err(|e| StorageError::PresignFailed(e.to_string()))?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presign_config)
            .await
            .map_err(|e| StorageError::PresignFailed(e.to_string()))?;

        Ok(presigned.uri().to_string())
    }

    /// Delete an object.
    pub async fn delete_object(&self, key: &str) -> StorageResult<()> {
        debug!("Deleting {}", key);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::delete_failed(e.to_string()))?;

        Ok(())
    }

    /// Delete multiple objects.
    pub async fn delete_objects(&self, keys: &[String]) -> StorageResult<u32> {
        if keys.is_empty() {
            return Ok(0);
        }

        debug!("Deleting {} objects", keys.len());

        let objects: Vec<_> = keys
            .iter()
            .map(|k| {
                aws_sdk_s3::types::ObjectIdentifier::builder()
                    .key(k)
                    .build()
                    .expect("valid key")
            })
            .collect();

        let delete = aws_sdk_s3::types::Delete::builder()
            .set_objects(Some(objects))
            .quiet(true)
            .build()
            .map_err(|e| StorageError::delete_failed(e.to_string()))?;

        self.client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| StorageError::delete_failed(e.to_string()))?;

        info!("Deleted {} objects", keys.len());
        Ok(keys.len() as u32)
    }

    /// List objects with a prefix.
    pub async fn list_objects(&self, prefix: &str) -> StorageResult<Vec<ObjectInfo>> {
        debug!("Listing objects with prefix: {}", prefix);

        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| StorageError::ListFailed(e.to_string()))?;

            if let Some(ref contents) = response.contents {
                for obj in contents {
                    objects.push(ObjectInfo {
                        key: obj.key.clone().unwrap_or_default(),
                        size: obj.size.unwrap_or(0) as u64,
                        last_modified: obj
                            .last_modified
                            .as_ref()
                            .and_then(|t| t.to_millis().ok())
                            .map(|ms| ms as u64),
                    });
                }
            }

            if response.is_truncated() == Some(true) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(objects)
    }

    /// Check if an object exists.
    pub async fn exists(&self, key: &str) -> StorageResult<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("NotFound") || e.to_string().contains("NoSuchKey") {
                    Ok(false)
                } else {
                    Err(StorageError::AwsSdk(e.to_string()))
                }
            }
        }
    }

    /// Check connectivity to R2 by performing a head bucket operation.
    pub async fn check_connectivity(&self) -> StorageResult<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| StorageError::AwsSdk(format!("R2 connectivity check failed: {}", e)))?;
        Ok(())
    }
}

/// Information about a stored object.
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    /// Object key
    pub key: String,
    /// Size in bytes
    pub size: u64,
    /// Last modified timestamp (milliseconds since epoch)
    pub last_modified: Option<u64>,
}
