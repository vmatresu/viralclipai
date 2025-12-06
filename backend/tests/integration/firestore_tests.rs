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

    // Create a test video
    let video_id = VideoId::new();
    let video = VideoMetadata {
        id: video_id.clone(),
        user_id: user_id.to_string(),
        video_url: "https://www.youtube.com/watch?v=test".to_string(),
        video_title: "Integration Test Video".to_string(),
        status: VideoStatus::Processing,
        clips_count: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        completed_at: None,
        error_message: None,
        custom_prompt: None,
    };

    // Create
    repo.create(&video).await.expect("Failed to create video");
    println!("Created video: {}", video_id);

    // Read
    let fetched = repo.get(video_id.as_str()).await.expect("Failed to get video");
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.video_title, "Integration Test Video");

    // Update status
    repo.update_status(video_id.as_str(), VideoStatus::Completed)
        .await
        .expect("Failed to update status");

    // Verify update
    let updated = repo.get(video_id.as_str()).await.expect("Failed to get video").unwrap();
    assert_eq!(updated.status, VideoStatus::Completed);

    // Delete
    repo.delete(video_id.as_str()).await.expect("Failed to delete video");
    println!("Deleted video: {}", video_id);

    // Verify deletion
    let deleted = repo.get(video_id.as_str()).await.expect("Failed to get video");
    assert!(deleted.is_none());
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
