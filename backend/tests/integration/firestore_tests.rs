//! Firestore integration tests.

/// Test Firestore connection.
#[tokio::test]
#[ignore = "requires Firestore credentials"]
async fn test_firestore_connection() {
    dotenvy::dotenv().ok();

    let client = vclip_firestore::FirestoreClient::from_env()
        .await
        .expect("Failed to create Firestore client");

    // Test health check document read (should return NotFound, which is OK)
    let result = client.get_document("_health", "_check").await;
    match result {
        Ok(_) => println!("Health check document exists"),
        Err(e) if e.to_string().contains("NOT_FOUND") || e.to_string().contains("404") => {
            println!("Health check document not found (expected)");
        }
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

/// Test video repository CRUD operations.
#[tokio::test]
#[ignore = "requires Firestore credentials"]
async fn test_video_repository() {
    use vclip_firestore::VideoRepository;
    use vclip_models::{VideoId, VideoMetadata, VideoStatus};

    dotenvy::dotenv().ok();

    let client = vclip_firestore::FirestoreClient::from_env()
        .await
        .expect("Failed to create Firestore client");

    let user_id = "test_user_integration";
    let repo = VideoRepository::new(client.clone(), user_id);

    // Create a few test videos to validate pagination and batch status reads.
    let video_ids: Vec<VideoId> = (0..3).map(|_| VideoId::new()).collect();

    for (i, video_id) in video_ids.iter().enumerate() {
        let video = VideoMetadata {
            video_id: video_id.clone(),
            user_id: user_id.to_string(),
            video_url: "https://www.youtube.com/watch?v=test".to_string(),
            video_title: format!("Integration Test Video {}", i),
            youtube_id: "test".to_string(),
            status: VideoStatus::Processing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            failed_at: None,
            error_message: None,
            highlights_count: 0,
            custom_prompt: None,
            styles_processed: vec![],
            crop_mode: "none".to_string(),
            target_aspect: "9:16".to_string(),
            clips_count: 0,
            total_size_bytes: 0,
            clips_by_style: std::collections::HashMap::new(),
            highlights_json_key: "test".to_string(),
            created_by: user_id.to_string(),
            source_video_r2_key: None,
            source_video_status: None,
            source_video_expires_at: None,
            source_video_error: None,
        };
        repo.create(&video).await.expect("Failed to create video");
        println!("Created video: {}", video_id);
    }

    // Read one back
    let fetched = repo
        .get(&video_ids[0])
        .await
        .expect("Failed to get video");
    assert!(fetched.is_some());

    // Pagination: request 1 item per page, expect a next_page_token.
    let (page1, token1) = repo
        .list_page(Some(1), None)
        .await
        .expect("Failed to list_page");
    assert_eq!(page1.len(), 1);
    assert!(token1.is_some());

    let (page2, _token2) = repo
        .list_page(Some(1), token1.as_deref())
        .await
        .expect("Failed to list_page (page 2)");
    assert_eq!(page2.len(), 1);

    // Batch status snapshots
    let snapshots = repo
        .get_status_snapshots(&video_ids)
        .await
        .expect("Failed to get_status_snapshots");
    assert!(snapshots.len() <= video_ids.len());

    // Update status of first video
    repo.update_status(&video_ids[0], VideoStatus::Completed)
        .await
        .expect("Failed to update status");

    // Verify update
    let updated = repo
        .get(&video_ids[0])
        .await
        .expect("Failed to get video")
        .unwrap();
    assert_eq!(updated.status, VideoStatus::Completed);

    // Cleanup
    for video_id in &video_ids {
        repo.delete(video_id).await.expect("Failed to delete video");
        println!("Deleted video: {}", video_id);
    }
}

/// Test user repository operations.
#[tokio::test]
#[ignore = "requires Firestore credentials"]
async fn test_user_repository() {
    use vclip_api::services::UserService;

    dotenvy::dotenv().ok();

    let firestore_client = vclip_firestore::FirestoreClient::from_env()
        .await
        .expect("Failed to create Firestore client");
    
    let storage_client = vclip_storage::R2Client::from_env()
        .await
        .expect("Failed to create Storage client");

    let repo = UserService::new(firestore_client, storage_client);

    let user_id = "test_user_integration_user";

    // Get or create user
    let user = repo
        .get_or_create_user(user_id, Some("test@example.com"))
        .await
        .expect("Failed to get or create user");

    println!("User: {:?}", user);
    assert_eq!(user.uid, user_id);

    // Check plan
    let has_pro = repo.has_pro_or_studio(user_id).await.expect("Failed to check plan");
    println!("Has pro/studio: {}", has_pro);
}
