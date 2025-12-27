//! Filesystem utilities for cross-device file operations.
//!
//! This module provides utilities for moving files that may be on different
//! filesystems, handling the EXDEV error gracefully.

use std::path::Path;
use tokio::fs;

use crate::error::{MediaError, MediaResult};

/// Move a file from `src` to `dst`, handling cross-device moves.
///
/// This function first attempts a fast rename. If that fails with EXDEV
/// (cross-device link error), it falls back to copy-and-delete.
///
/// The copy is performed to a temporary file first, then renamed to the
/// destination to ensure atomicity on the destination filesystem.
///
/// # Errors
///
/// Returns an error if:
/// - The source file doesn't exist
/// - The destination directory doesn't exist and can't be created
/// - The copy or rename operations fail
///
/// # Example
///
/// ```ignore
/// use vclip_media::fs_utils::move_file;
///
/// move_file("/tmp/video.mp4", "/mnt/storage/video.mp4").await?;
/// ```
pub async fn move_file(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> MediaResult<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    // Create parent directory if needed before attempting rename
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await?;
        }
    }

    match fs::rename(src, dst).await {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device_error(&e) => {
            // Cross-device move: copy then delete
            tracing::debug!(
                "Cross-device rename detected, falling back to copy+delete: {} -> {}",
                src.display(),
                dst.display()
            );
            copy_and_delete(src, dst).await
        }
        Err(e) => Err(MediaError::from(e)),
    }
}

/// Check if an IO error is EXDEV (cross-device link).
fn is_cross_device_error(e: &std::io::Error) -> bool {
    // EXDEV is error code 18 on Linux/macOS
    e.raw_os_error() == Some(18)
}

/// Copy file to destination (via temp file) then delete source.
async fn copy_and_delete(src: &Path, dst: &Path) -> MediaResult<()> {
    // Create parent directory if needed
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await?;
        }
    }

    // Copy to a temp file in the same directory as dst (ensures same filesystem)
    let tmp_dst = dst.with_extension("tmp");

    fs::copy(src, &tmp_dst).await.map_err(|e| {
        tracing::error!(
            "Failed to copy file during cross-device move: {} -> {}: {}",
            src.display(),
            tmp_dst.display(),
            e
        );
        MediaError::from(e)
    })?;

    // Atomic rename on destination filesystem
    fs::rename(&tmp_dst, dst).await.map_err(|e| {
        // Clean up temp file on failure
        let _ = std::fs::remove_file(&tmp_dst);
        tracing::error!(
            "Failed to rename temp file during cross-device move: {} -> {}: {}",
            tmp_dst.display(),
            dst.display(),
            e
        );
        MediaError::from(e)
    })?;

    // Delete source (best effort - log failure but don't fail the operation)
    if let Err(e) = fs::remove_file(src).await {
        tracing::warn!(
            "Failed to remove source file after cross-device move: {}: {}",
            src.display(),
            e
        );
    }

    tracing::debug!(
        "Successfully completed cross-device move: {} -> {}",
        src.display(),
        dst.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_move_file_same_filesystem() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source.txt");
        let dst = dir.path().join("dest.txt");

        fs::write(&src, b"test content").await.unwrap();

        move_file(&src, &dst).await.unwrap();

        assert!(!src.exists(), "Source file should be removed");
        assert!(dst.exists(), "Destination file should exist");
        assert_eq!(fs::read_to_string(&dst).await.unwrap(), "test content");
    }

    #[tokio::test]
    async fn test_move_file_to_subdirectory() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source.txt");
        let dst = dir.path().join("subdir").join("dest.txt");

        fs::write(&src, b"test content").await.unwrap();

        move_file(&src, &dst).await.unwrap();

        assert!(!src.exists());
        assert!(dst.exists());
    }

    #[tokio::test]
    async fn test_move_file_overwrites_destination() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source.txt");
        let dst = dir.path().join("dest.txt");

        fs::write(&src, b"new content").await.unwrap();
        fs::write(&dst, b"old content").await.unwrap();

        move_file(&src, &dst).await.unwrap();

        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).await.unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_is_cross_device_error() {
        // Test that we correctly identify EXDEV
        let exdev_error = std::io::Error::from_raw_os_error(18);
        assert!(is_cross_device_error(&exdev_error));

        // Other errors should not match
        let not_found = std::io::Error::from_raw_os_error(2);
        assert!(!is_cross_device_error(&not_found));
    }
}
