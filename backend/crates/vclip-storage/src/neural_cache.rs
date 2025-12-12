//! Neural analysis cache helpers.
//!
//! This module provides functions to store and retrieve `SceneNeuralAnalysis`
//! data in R2 as gzip-compressed JSON files. This allows caching expensive
//! ML inference results (YuNet face detection, FaceMesh landmarks) across
//! multiple rendering passes.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use tracing::{debug, warn};

use crate::client::R2Client;
use crate::error::{StorageError, StorageResult};
use vclip_models::{SceneNeuralAnalysis, NEURAL_ANALYSIS_VERSION};

/// Content type for gzip-compressed JSON.
const CONTENT_TYPE_GZIP: &str = "application/gzip";

/// Generate the R2 key for a neural analysis cache entry.
///
/// Format: `{user_id}/{video_id}/neural/{scene_id}.json.gz`
pub fn neural_cache_key(user_id: &str, video_id: &str, scene_id: u32) -> String {
    format!("{}/{}/neural/{}.json.gz", user_id, video_id, scene_id)
}

/// Compress `SceneNeuralAnalysis` to gzip JSON bytes.
///
/// Returns the compressed bytes ready for upload to R2.
pub fn compress_neural_analysis(analysis: &SceneNeuralAnalysis) -> StorageResult<Vec<u8>> {
    let json = serde_json::to_string(analysis).map_err(|e| {
        StorageError::Serialization(format!("Failed to serialize neural analysis: {}", e))
    })?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json.as_bytes()).map_err(|e| {
        StorageError::Serialization(format!("Failed to gzip neural analysis: {}", e))
    })?;

    encoder.finish().map_err(|e| {
        StorageError::Serialization(format!("Failed to finish gzip encoding: {}", e))
    })
}

/// Decompress gzip JSON bytes to `SceneNeuralAnalysis`.
///
/// Returns `None` if decompression or deserialization fails (treated as cache miss).
/// Also returns `None` if the cached version is outdated.
pub fn decompress_neural_analysis(data: &[u8]) -> Option<SceneNeuralAnalysis> {
    // Attempt to decompress
    let mut decoder = GzDecoder::new(data);
    let mut json = String::new();

    if let Err(e) = decoder.read_to_string(&mut json) {
        warn!(error = %e, "Failed to decompress neural analysis cache");
        return None;
    }

    // Attempt to deserialize
    match serde_json::from_str::<SceneNeuralAnalysis>(&json) {
        Ok(analysis) => {
            // Check version compatibility
            if analysis.is_current_version() {
                Some(analysis)
            } else {
                debug!(
                    cached_version = analysis.analysis_version,
                    current_version = NEURAL_ANALYSIS_VERSION,
                    "Neural analysis cache version mismatch, treating as miss"
                );
                None
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to deserialize neural analysis cache");
            None
        }
    }
}

/// Result of storing neural analysis, including actual compressed size.
pub struct StoreResult {
    /// R2 key where the analysis was stored
    pub key: String,
    /// Actual compressed size in bytes
    pub compressed_size: u64,
}

/// Store neural analysis to R2.
///
/// The data is compressed with gzip before upload.
/// Returns the R2 key and actual compressed size for accurate storage accounting.
pub async fn store_neural_analysis(
    r2: &R2Client,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
    analysis: &SceneNeuralAnalysis,
) -> StorageResult<StoreResult> {
    let key = neural_cache_key(user_id, video_id, scene_id);
    let compressed = compress_neural_analysis(analysis)?;
    let compressed_size = compressed.len() as u64;

    debug!(
        key = %key,
        frames = analysis.frames.len(),
        compressed_size = compressed_size,
        "Storing neural analysis to R2"
    );

    r2.upload_bytes(compressed, &key, CONTENT_TYPE_GZIP).await?;

    Ok(StoreResult {
        key,
        compressed_size,
    })
}

/// Load neural analysis from R2.
///
/// Returns `None` if:
/// - The key doesn't exist
/// - Decompression fails (corrupt data)
/// - Deserialization fails (schema mismatch)
/// - Version is outdated
///
/// All of these cases are treated as cache misses.
pub async fn load_neural_analysis(
    r2: &R2Client,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
) -> Option<SceneNeuralAnalysis> {
    let key = neural_cache_key(user_id, video_id, scene_id);

    // Try to download
    let data = match r2.download_bytes(&key).await {
        Ok(data) => data,
        Err(e) => {
            // NotFound or other error = cache miss
            debug!(key = %key, error = %e, "Neural analysis cache miss (download failed)");
            return None;
        }
    };

    // Try to decompress and deserialize
    match decompress_neural_analysis(&data) {
        Some(analysis) => {
            debug!(
                key = %key,
                frames = analysis.frames.len(),
                "Neural analysis cache hit"
            );
            Some(analysis)
        }
        None => {
            debug!(key = %key, "Neural analysis cache miss (corrupt or outdated)");
            None
        }
    }
}

/// Check if neural analysis exists in cache.
///
/// This is a lightweight check that doesn't download the full data.
/// Note: This doesn't verify the data is valid or current version.
pub async fn neural_analysis_exists(
    r2: &R2Client,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
) -> bool {
    let key = neural_cache_key(user_id, video_id, scene_id);
    r2.exists(&key).await.unwrap_or(false)
}

/// Delete neural analysis from cache.
///
/// This can be used to invalidate cache entries when source video changes.
pub async fn delete_neural_analysis(
    r2: &R2Client,
    user_id: &str,
    video_id: &str,
    scene_id: u32,
) -> StorageResult<()> {
    let key = neural_cache_key(user_id, video_id, scene_id);
    r2.delete_object(&key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::{BoundingBox, FaceDetection, FrameAnalysis};

    fn create_test_analysis() -> SceneNeuralAnalysis {
        let mut analysis = SceneNeuralAnalysis::new("test_video", 1).with_user("test_user");

        // Add some frames with faces
        for i in 0..10 {
            let mut frame = FrameAnalysis::new(i as f64 * 0.1);
            frame.add_face(
                FaceDetection::new(BoundingBox::new(0.3, 0.2, 0.4, 0.5), 0.95)
                    .with_track_id(1)
                    .with_mouth_openness(0.3),
            );
            analysis.add_frame(frame);
        }

        analysis
    }

    #[test]
    fn test_neural_cache_key() {
        let key = neural_cache_key("user123", "video456", 7);
        assert_eq!(key, "user123/video456/neural/7.json.gz");
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let analysis = create_test_analysis();

        // Compress
        let compressed = compress_neural_analysis(&analysis).expect("compress should succeed");
        assert!(!compressed.is_empty());

        // Verify compression happened (should be smaller than raw JSON)
        let json = serde_json::to_string(&analysis).unwrap();
        assert!(
            compressed.len() < json.len(),
            "Compressed size {} should be less than JSON size {}",
            compressed.len(),
            json.len()
        );

        // Decompress
        let decompressed =
            decompress_neural_analysis(&compressed).expect("decompress should succeed");

        // Verify equality
        assert_eq!(analysis.video_id, decompressed.video_id);
        assert_eq!(analysis.scene_id, decompressed.scene_id);
        assert_eq!(analysis.frames.len(), decompressed.frames.len());
        assert_eq!(
            analysis.frames[0].faces[0].score,
            decompressed.frames[0].faces[0].score
        );
    }

    #[test]
    fn test_decompress_corrupt_data() {
        let corrupt_data = b"not gzip data at all";
        let result = decompress_neural_analysis(corrupt_data);
        assert!(result.is_none(), "Corrupt data should return None");
    }

    #[test]
    fn test_decompress_invalid_json() {
        // Create valid gzip but invalid JSON
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"{ invalid json }").unwrap();
        let compressed = encoder.finish().unwrap();

        let result = decompress_neural_analysis(&compressed);
        assert!(result.is_none(), "Invalid JSON should return None");
    }

    #[test]
    fn test_decompress_outdated_version() {
        let mut analysis = create_test_analysis();
        analysis.analysis_version = 0; // Outdated version

        let compressed = compress_neural_analysis(&analysis).expect("compress should succeed");
        let result = decompress_neural_analysis(&compressed);
        assert!(
            result.is_none(),
            "Outdated version should return None (cache miss)"
        );
    }

    #[test]
    fn test_empty_analysis() {
        let analysis = SceneNeuralAnalysis::new("video", 0);

        let compressed = compress_neural_analysis(&analysis).expect("compress should succeed");
        let decompressed =
            decompress_neural_analysis(&compressed).expect("decompress should succeed");

        assert!(decompressed.frames.is_empty());
        assert!(decompressed.user_id.is_none());
    }
}
