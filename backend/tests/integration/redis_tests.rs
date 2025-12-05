//! Redis/Queue integration tests.

use std::time::Duration;

/// Test Redis connection and basic operations.
#[tokio::test]
#[ignore = "requires Redis"]
async fn test_redis_connection() {
    dotenvy::dotenv().ok();

    let queue = vclip_queue::JobQueue::from_env().expect("Failed to create queue");
    queue.init().await.expect("Failed to initialize queue");

    // Test queue length (should not error)
    let len = queue.len().await.expect("Failed to get queue length");
    println!("Queue length: {}", len);
}

/// Test job enqueue and dequeue cycle.
#[tokio::test]
#[ignore = "requires Redis"]
async fn test_job_enqueue_dequeue() {
    use vclip_models::Style;
    use vclip_queue::ProcessVideoJob;

    dotenvy::dotenv().ok();

    let queue = vclip_queue::JobQueue::from_env().expect("Failed to create queue");
    queue.init().await.expect("Failed to initialize queue");

    // Create a test job
    let job = ProcessVideoJob::new(
        "test_user_123",
        "https://www.youtube.com/watch?v=test",
        vec![Style::Split],
    );
    let job_id = job.job_id.clone();

    // Enqueue
    let message_id = queue.enqueue_process(job).await.expect("Failed to enqueue");
    println!("Enqueued job {} with message ID {}", job_id, message_id);

    // Consume
    let consumer_name = "test-consumer";
    let jobs = queue
        .consume(consumer_name, 1000, 1)
        .await
        .expect("Failed to consume");

    assert_eq!(jobs.len(), 1);
    let (msg_id, consumed_job) = &jobs[0];
    assert_eq!(consumed_job.job_id(), &job_id);

    // Acknowledge
    queue.ack(msg_id).await.expect("Failed to ack");
    println!("Job {} acknowledged", job_id);
}

/// Test DLQ functionality.
#[tokio::test]
#[ignore = "requires Redis"]
async fn test_dlq() {
    use vclip_models::Style;
    use vclip_queue::{ProcessVideoJob, QueueJob};

    dotenvy::dotenv().ok();

    let queue = vclip_queue::JobQueue::from_env().expect("Failed to create queue");
    queue.init().await.expect("Failed to initialize queue");

    // Create and enqueue a job
    let job = ProcessVideoJob::new(
        "test_dlq_user",
        "https://www.youtube.com/watch?v=dlq_test",
        vec![Style::Split],
    );
    let job_id = job.job_id.clone();

    let message_id = queue.enqueue_process(job.clone()).await.expect("Failed to enqueue");

    // Consume it
    let consumer_name = "test-dlq-consumer";
    let jobs = queue.consume(consumer_name, 1000, 1).await.expect("Failed to consume");
    assert!(!jobs.is_empty());

    // Move to DLQ
    let queue_job = QueueJob::ProcessVideo(job);
    queue
        .dlq(&message_id, &queue_job, "Test error")
        .await
        .expect("Failed to move to DLQ");

    // Check DLQ length increased
    let dlq_len = queue.dlq_len().await.expect("Failed to get DLQ length");
    assert!(dlq_len > 0);
    println!("DLQ length: {}", dlq_len);
}

/// Test progress channel pub/sub.
#[tokio::test]
#[ignore = "requires Redis"]
async fn test_progress_channel() {
    use futures_util::StreamExt;
    use vclip_models::JobId;

    dotenvy::dotenv().ok();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let progress = vclip_queue::ProgressChannel::new(&redis_url).expect("Failed to create progress channel");

    let job_id = JobId::new();

    // Subscribe in a separate task
    let progress_clone = progress.clone();
    let job_id_clone = job_id.clone();
    let subscriber = tokio::spawn(async move {
        let mut stream = progress_clone.subscribe(&job_id_clone).await.expect("Failed to subscribe");
        let mut messages = Vec::new();
        
        // Collect messages with timeout
        let timeout = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(event) = stream.next().await {
                messages.push(event);
                if messages.len() >= 2 {
                    break;
                }
            }
        });
        
        let _ = timeout.await;
        messages
    });

    // Give subscriber time to connect
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish some events
    progress.log(&job_id, "Test message 1").await.ok();
    progress.progress(&job_id, 50).await.ok();

    // Wait for subscriber
    let messages = subscriber.await.expect("Subscriber task failed");
    println!("Received {} messages", messages.len());
}
