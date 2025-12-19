//! Transcript cache helpers.
//!
//! Stores parsed transcript text in R2 as gzip-compressed bytes to avoid
//! re-downloading captions with yt-dlp.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::client::R2Client;
use crate::error::{StorageError, StorageResult};
use vclip_models::extract_youtube_id;

/// Content type for gzip-compressed text.
const CONTENT_TYPE_GZIP: &str = "application/gzip";

/// Generate a stable cache ID from a video URL.
///
/// - Uses YouTube ID when available
/// - Falls back to SHA-256 hash of the URL
pub fn transcript_cache_id_from_url(video_url: &str) -> String {
    if let Ok(id) = extract_youtube_id(video_url) {
        return id;
    }

    let digest = Sha256::digest(video_url.trim().as_bytes());
    format!("{:x}", digest)
}

/// Generate the R2 key for a transcript cache entry.
///
/// Format: `{user_id}/transcripts/{cache_id}.txt.gz`
pub fn transcript_cache_key(user_id: &str, cache_id: &str) -> String {
    format!("{}/transcripts/{}.txt.gz", user_id, cache_id)
}

/// Compress transcript text to gzip bytes.
pub fn compress_transcript(transcript: &str) -> StorageResult<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(transcript.as_bytes()).map_err(|e| {
        StorageError::Serialization(format!("Failed to gzip transcript: {}", e))
    })?;

    encoder.finish().map_err(|e| {
        StorageError::Serialization(format!("Failed to finish gzip encoding: {}", e))
    })
}

/// Decompress gzip bytes to transcript text.
///
/// Returns `None` if decompression fails (treated as cache miss).
pub fn decompress_transcript(data: &[u8]) -> Option<String> {
    let mut decoder = GzDecoder::new(data);
    let mut text = String::new();

    if let Err(e) = decoder.read_to_string(&mut text) {
        warn!(error = %e, "Failed to decompress transcript cache");
        return None;
    }

    Some(text)
}

/// Result of storing transcript, including actual compressed size.
pub struct StoreResult {
    /// R2 key where the transcript was stored
    pub key: String,
    /// Actual compressed size in bytes
    pub compressed_size: u64,
}

/// Store transcript to R2 (gzip-compressed).
pub async fn store_transcript(
    r2: &R2Client,
    user_id: &str,
    cache_id: &str,
    transcript: &str,
) -> StorageResult<StoreResult> {
    let key = transcript_cache_key(user_id, cache_id);
    let compressed = compress_transcript(transcript)?;
    let compressed_size = compressed.len() as u64;

    debug!(
        key = %key,
        compressed_size = compressed_size,
        "Storing transcript to R2"
    );

    r2.upload_bytes(compressed, &key, CONTENT_TYPE_GZIP).await?;

    Ok(StoreResult {
        key,
        compressed_size,
    })
}

/// Load transcript from R2.
///
/// Returns `None` if:
/// - The key doesn't exist
/// - Decompression fails (corrupt data)
pub async fn load_transcript(
    r2: &R2Client,
    user_id: &str,
    cache_id: &str,
) -> Option<String> {
    let key = transcript_cache_key(user_id, cache_id);

    let data = match r2.download_bytes(&key).await {
        Ok(data) => data,
        Err(e) => {
            debug!(key = %key, error = %e, "Transcript cache miss (download failed)");
            return None;
        }
    };

    match decompress_transcript(&data) {
        Some(transcript) => {
            debug!(key = %key, "Transcript cache hit");
            Some(transcript)
        }
        None => {
            debug!(key = %key, "Transcript cache miss (corrupt data)");
            None
        }
    }
}

/// Check if transcript exists in cache.
pub async fn transcript_exists(r2: &R2Client, user_id: &str, cache_id: &str) -> bool {
    let key = transcript_cache_key(user_id, cache_id);
    r2.exists(&key).await.unwrap_or(false)
}

/// Delete transcript from cache.
pub async fn delete_transcript(
    r2: &R2Client,
    user_id: &str,
    cache_id: &str,
) -> StorageResult<()> {
    let key = transcript_cache_key(user_id, cache_id);
    r2.delete_object(&key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_cache_key() {
        let key = transcript_cache_key("user123", "video456");
        assert_eq!(key, "user123/transcripts/video456.txt.gz");
    }

    #[test]
    fn test_transcript_cache_id_from_url_youtube() {
        let id = transcript_cache_id_from_url("https://youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_transcript_cache_id_from_url_hash() {
        let id = transcript_cache_id_from_url("https://example.com/video?id=123");
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let transcript = "[00:00:01] Hello world\n[00:00:02] Another line\n";
        let compressed = compress_transcript(transcript).expect("compress should succeed");
        assert!(!compressed.is_empty());

        let decompressed = decompress_transcript(&compressed).expect("decompress should succeed");
        assert_eq!(transcript, decompressed);
    }

    #[test]
    fn test_decompress_corrupt_data() {
        let corrupt_data = b"not gzip data at all";
        let result = decompress_transcript(corrupt_data);
        assert!(result.is_none(), "Corrupt data should return None");
    }

    #[test]
    fn test_empty_transcript_roundtrip() {
        let transcript = "";
        let compressed = compress_transcript(transcript).expect("compress should succeed");
        let decompressed = decompress_transcript(&compressed).expect("decompress should succeed");
        assert_eq!(transcript, decompressed);
    }
}
