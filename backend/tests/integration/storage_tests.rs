//! R2 Storage integration tests.

/// Test R2 connection and bucket access.
#[tokio::test]
#[ignore = "requires R2 credentials"]
async fn test_r2_connection() {
    dotenvy::dotenv().ok();

    let client = vclip_storage::R2Client::from_env()
        .await
        .expect("Failed to create R2 client");

    // Test connectivity
    client
        .check_connectivity()
        .await
        .expect("Failed to check R2 connectivity");

    println!("R2 connectivity check passed");
}

/// Test presigned URL generation.
#[tokio::test]
#[ignore = "requires R2 credentials"]
async fn test_presigned_url() {
    dotenvy::dotenv().ok();

    let client = vclip_storage::R2Client::from_env()
        .await
        .expect("Failed to create R2 client");

    // Generate a presigned URL for a test key
    let url = client
        .presign_get("test/integration/test.mp4", std::time::Duration::from_secs(3600))
        .await
        .expect("Failed to generate presigned URL");

    println!("Presigned URL: {}", url);
    assert!(url.contains("X-Amz-Signature"));
}

/// Test file upload and download cycle.
#[tokio::test]
#[ignore = "requires R2 credentials"]
async fn test_upload_download() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    dotenvy::dotenv().ok();

    let client = vclip_storage::R2Client::from_env()
        .await
        .expect("Failed to create R2 client");

    // Create a test file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file
        .write_all(b"Integration test content")
        .expect("Failed to write to temp file");
    let temp_path = temp_file.path();

    let user_id = "test_user";
    let video_id = "test_video";
    let filename = "integration_test.txt";

    // Upload
    let key = client
        .upload_clip(temp_path, user_id, video_id, filename)
        .await
        .expect("Failed to upload file");

    println!("Uploaded to key: {}", key);

    // List files
    let files = client
        .list_files(user_id, video_id)
        .await
        .expect("Failed to list files");

    assert!(files.iter().any(|f| f.contains("integration_test.txt")));
    println!("Found {} files", files.len());

    // Delete
    client
        .delete_file(&key)
        .await
        .expect("Failed to delete file");

    println!("Deleted file: {}", key);
}

/// Test highlights JSON storage.
#[tokio::test]
#[ignore = "requires R2 credentials"]
async fn test_highlights_storage() {
    use vclip_models::Highlight;

    dotenvy::dotenv().ok();

    let client = vclip_storage::R2Client::from_env()
        .await
        .expect("Failed to create R2 client");

    let user_id = "test_user";
    let video_id = "test_video_highlights";

    // Create test highlights
    let highlights = vclip_storage::HighlightsData {
        video_url: "https://www.youtube.com/watch?v=test".to_string(),
        video_title: "Test Video".to_string(),
        highlights: vec![
            Highlight {
                id: 1,
                title: "Test Highlight 1".to_string(),
                start: "00:00:10".to_string(),
                end: "00:00:30".to_string(),
                duration: 20,
                hook_category: Some("hook".to_string()),
                reason: Some("Test reason".to_string()),
                description: None,
            },
        ],
    };

    // Save
    client
        .save_highlights(user_id, video_id, &highlights)
        .await
        .expect("Failed to save highlights");

    println!("Saved highlights for video: {}", video_id);

    // Load
    let loaded = client
        .load_highlights(user_id, video_id)
        .await
        .expect("Failed to load highlights");

    assert_eq!(loaded.highlights.len(), 1);
    assert_eq!(loaded.highlights[0].title, "Test Highlight 1");

    println!("Loaded {} highlights", loaded.highlights.len());
}
