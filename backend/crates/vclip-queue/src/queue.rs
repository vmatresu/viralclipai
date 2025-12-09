//! Job queue using Redis Streams.

use std::time::Duration;

use redis::AsyncCommands;
use tracing::{debug, info, warn};

use crate::error::{QueueError, QueueResult};
use crate::job::{ProcessVideoJob, QueueJob, RenderSceneStyleJob, ReprocessScenesJob};

/// Queue configuration.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Redis URL
    pub redis_url: String,
    /// Stream name for jobs
    pub stream_name: String,
    /// Consumer group name
    pub consumer_group: String,
    /// Dead letter queue stream name
    pub dlq_stream_name: String,
    /// Max retries before DLQ
    pub max_retries: u32,
    /// Job visibility timeout
    pub visibility_timeout: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            stream_name: "vclip:jobs".to_string(),
            consumer_group: "vclip:workers".to_string(),
            dlq_stream_name: "vclip:dlq".to_string(),
            max_retries: 3,
            visibility_timeout: Duration::from_secs(600), // 10 minutes
        }
    }
}

impl QueueConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Self {
        Self {
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            stream_name: std::env::var("QUEUE_STREAM")
                .unwrap_or_else(|_| "vclip:jobs".to_string()),
            consumer_group: std::env::var("QUEUE_CONSUMER_GROUP")
                .unwrap_or_else(|_| "vclip:workers".to_string()),
            dlq_stream_name: std::env::var("QUEUE_DLQ_STREAM")
                .unwrap_or_else(|_| "vclip:dlq".to_string()),
            max_retries: std::env::var("QUEUE_MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            visibility_timeout: Duration::from_secs(
                std::env::var("QUEUE_VISIBILITY_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600),
            ),
        }
    }
}

/// Job queue client.
pub struct JobQueue {
    client: redis::Client,
    config: QueueConfig,
}

impl JobQueue {
    /// Create a new job queue.
    pub fn new(config: QueueConfig) -> QueueResult<Self> {
        let client = redis::Client::open(config.redis_url.as_str())?;
        Ok(Self { client, config })
    }

    /// Create from environment variables.
    pub fn from_env() -> QueueResult<Self> {
        Self::new(QueueConfig::from_env())
    }

    /// Initialize the queue (create consumer group if not exists).
    pub async fn init(&self) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Create consumer group (ignore error if already exists)
        let result: Result<(), redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .arg("$")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;

        match result {
            Ok(_) => info!("Created consumer group: {}", self.config.consumer_group),
            Err(e) if e.to_string().contains("BUSYGROUP") => {
                debug!("Consumer group already exists: {}", self.config.consumer_group);
            }
            Err(e) => return Err(QueueError::Redis(e)),
        }

        Ok(())
    }

    /// Enqueue a process video job.
    pub async fn enqueue_process(&self, job: ProcessVideoJob) -> QueueResult<String> {
        self.enqueue(QueueJob::ProcessVideo(job)).await
    }

    /// Enqueue a reprocess scenes job.
    pub async fn enqueue_reprocess(&self, job: ReprocessScenesJob) -> QueueResult<String> {
        self.enqueue(QueueJob::ReprocessScenes(job)).await
    }

    /// Enqueue a single render scene/style job.
    pub async fn enqueue_render(&self, job: RenderSceneStyleJob) -> QueueResult<String> {
        self.enqueue(QueueJob::RenderSceneStyle(job)).await
    }

    /// Enqueue multiple render jobs efficiently.
    ///
    /// Returns the message IDs of all successfully enqueued jobs.
    /// If any job fails to enqueue (e.g., duplicate), it is skipped.
    pub async fn enqueue_render_batch(&self, jobs: Vec<RenderSceneStyleJob>) -> QueueResult<Vec<String>> {
        let mut message_ids = Vec::with_capacity(jobs.len());
        for job in jobs {
            match self.enqueue_render(job).await {
                Ok(id) => message_ids.push(id),
                Err(QueueError::EnqueueFailed { .. }) => {
                    // Skip duplicates but continue with others
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(message_ids)
    }

    /// Enqueue a job.
    async fn enqueue(&self, job: QueueJob) -> QueueResult<String> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let payload = serde_json::to_string(&job)?;
        let idempotency_key = job.idempotency_key();

        // Check for duplicate using idempotency key
        let dedup_key = format!("vclip:dedup:{}", idempotency_key);
        let exists: bool = conn.exists(&dedup_key).await?;
        if exists {
            warn!("Duplicate job rejected: {}", idempotency_key);
            return Err(QueueError::enqueue_failed("Duplicate job"));
        }

        // Add to stream
        let message_id: String = redis::cmd("XADD")
            .arg(&self.config.stream_name)
            .arg("*")
            .arg("job")
            .arg(&payload)
            .arg("key")
            .arg(&idempotency_key)
            .query_async(&mut conn)
            .await?;

        // Set dedup key with TTL (1 hour)
        conn.set_ex::<_, _, ()>(&dedup_key, "1", 3600).await?;

        info!(
            "Enqueued job {} with message ID {}",
            job.job_id(),
            message_id
        );

        Ok(message_id)
    }

    /// Acknowledge a job (mark as completed).
    pub async fn ack(&self, message_id: &str) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        redis::cmd("XACK")
            .arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .arg(message_id)
            .query_async::<()>(&mut conn)
            .await?;

        // Delete the message from the stream
        redis::cmd("XDEL")
            .arg(&self.config.stream_name)
            .arg(message_id)
            .query_async::<()>(&mut conn)
            .await?;

        debug!("Acknowledged job: {}", message_id);
        Ok(())
    }

    /// Clear the deduplication key for a job, allowing it to be reprocessed.
    /// Should be called after job completion (success or DLQ).
    pub async fn clear_dedup(&self, job: &QueueJob) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let idempotency_key = job.idempotency_key();
        let dedup_key = format!("vclip:dedup:{}", idempotency_key);
        conn.del::<_, ()>(&dedup_key).await?;
        debug!("Cleared dedup key: {}", dedup_key);
        Ok(())
    }

    /// Move a job to the dead letter queue.
    pub async fn dlq(&self, message_id: &str, job: &QueueJob, error: &str) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let payload = serde_json::to_string(job)?;

        // Add to DLQ
        redis::cmd("XADD")
            .arg(&self.config.dlq_stream_name)
            .arg("*")
            .arg("job")
            .arg(&payload)
            .arg("error")
            .arg(error)
            .arg("original_id")
            .arg(message_id)
            .query_async::<()>(&mut conn)
            .await?;

        // Ack the original message
        self.ack(message_id).await?;

        warn!("Moved job {} to DLQ: {}", job.job_id(), error);
        Ok(())
    }

    /// Get queue length.
    pub async fn len(&self) -> QueueResult<u64> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let len: u64 = conn.xlen(&self.config.stream_name).await?;
        Ok(len)
    }

    /// Get DLQ length.
    pub async fn dlq_len(&self) -> QueueResult<u64> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let len: u64 = conn.xlen(&self.config.dlq_stream_name).await?;
        Ok(len)
    }

    /// Consume jobs from the queue.
    /// Returns a stream of (message_id, job) pairs.
    pub async fn consume(
        &self,
        consumer_name: &str,
        block_ms: u64,
        count: usize,
    ) -> QueueResult<Vec<(String, QueueJob)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Read from consumer group
        let result: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.config.consumer_group)
            .arg(consumer_name)
            .arg("COUNT")
            .arg(count)
            .arg("BLOCK")
            .arg(block_ms)
            .arg("STREAMS")
            .arg(&self.config.stream_name)
            .arg(">") // Only new messages
            .query_async(&mut conn)
            .await?;

        let mut jobs = Vec::new();

        for stream_key in result.keys {
            for entry in stream_key.ids {
                let message_id = entry.id.clone();

                // Extract job payload
                if let Some(redis::Value::BulkString(payload)) = entry.map.get("job") {
                    let payload_str = String::from_utf8_lossy(payload);
                    match serde_json::from_str::<QueueJob>(&payload_str) {
                        Ok(job) => {
                            debug!("Consumed job {} from stream", job.job_id());
                            jobs.push((message_id, job));
                        }
                        Err(e) => {
                            warn!("Failed to parse job payload: {}", e);
                            // Ack the malformed message to prevent reprocessing
                            self.ack(&message_id).await.ok();
                        }
                    }
                }
            }
        }

        Ok(jobs)
    }

    /// Claim pending jobs that have been idle for too long.
    /// This handles jobs from crashed workers.
    pub async fn claim_pending(
        &self,
        consumer_name: &str,
        min_idle_ms: u64,
        count: usize,
    ) -> QueueResult<Vec<(String, QueueJob)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // First check if there are any pending messages
        let pending_count: usize = redis::cmd("XPENDING")
            .arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .query_async(&mut conn)
            .await
            .map(|reply: redis::streams::StreamPendingReply| reply.count())
            .unwrap_or(0);

        if pending_count == 0 {
            return Ok(Vec::new());
        }

        // Get detailed pending entries: XPENDING stream group start end count
        let pending_details: Vec<Vec<redis::Value>> = redis::cmd("XPENDING")
            .arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .arg("-")  // start from oldest
            .arg("+")  // end at newest
            .arg(count)  // limit count
            .query_async(&mut conn)
            .await?;

        // Parse pending details to extract message IDs that have been idle long enough
        let mut message_ids_to_claim = Vec::new();
        for detail in pending_details {
            if detail.len() >= 4 {
                // Format: [id, consumer, idle_time_ms, delivery_count]
                if let (Some(redis::Value::BulkString(id_bytes)), Some(redis::Value::Int(idle_ms))) =
                    (detail.get(0), detail.get(2))
                {
                    let idle_ms = *idle_ms as u64;
                    if idle_ms >= min_idle_ms {
                        if let Ok(id) = String::from_utf8(id_bytes.clone()) {
                            message_ids_to_claim.push(id);
                        }
                    }
                }
            }
        }

        if message_ids_to_claim.is_empty() {
            return Ok(Vec::new());
        }

        // Claim the specific message IDs using XCLAIM
        let mut cmd = redis::cmd("XCLAIM");
        cmd.arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .arg(consumer_name)
            .arg(min_idle_ms);

        // Add all message IDs to claim
        for msg_id in &message_ids_to_claim {
            cmd.arg(msg_id);
        }

        // XCLAIM returns an array of claimed messages
        let claimed_messages: Vec<Vec<redis::Value>> = cmd.query_async(&mut conn).await?;

        let mut jobs = Vec::new();

        for message in claimed_messages {
            if message.len() >= 2 {
                // Format: [id, [field1, value1, field2, value2, ...]]
                if let (Some(redis::Value::BulkString(id_bytes)), Some(redis::Value::Array(fields))) =
                    (message.get(0), message.get(1))
                {
                    if let Ok(message_id) = String::from_utf8(id_bytes.clone()) {
                        // Parse fields array to find the "job" field
                        let mut job_payload: Option<String> = None;
                        let mut i = 0;
                        while i < fields.len() - 1 {
                            if let (Some(redis::Value::BulkString(field_bytes)), Some(redis::Value::BulkString(value_bytes))) =
                                (fields.get(i), fields.get(i + 1))
                            {
                                if let (Ok(field), Ok(value)) = (
                                    String::from_utf8(field_bytes.clone()),
                                    String::from_utf8(value_bytes.clone())
                                ) {
                                    if field == "job" {
                                        job_payload = Some(value);
                                        break;
                                    }
                                }
                            }
                            i += 2;
                        }

                        if let Some(payload) = job_payload {
                            match serde_json::from_str::<QueueJob>(&payload) {
                                Ok(job) => {
                                    info!("Claimed pending job {} from stream", job.job_id());
                                    jobs.push((message_id, job));
                                }
                                Err(e) => {
                                    warn!("Failed to parse claimed job payload: {}", e);
                                    self.ack(&message_id).await.ok();
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(jobs)
    }

    /// Get retry count for a job from its metadata.
    pub async fn get_retry_count(&self, message_id: &str) -> QueueResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let key = format!("vclip:retry:{}", message_id);
        let count: Option<u32> = conn.get(&key).await?;
        Ok(count.unwrap_or(0))
    }

    /// Increment retry count for a job.
    pub async fn increment_retry(&self, message_id: &str) -> QueueResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let key = format!("vclip:retry:{}", message_id);
        let count: u32 = conn.incr(&key, 1).await?;
        // Set TTL to 24 hours
        conn.expire::<_, ()>(&key, 86400).await?;
        Ok(count)
    }

    /// Get max retries from config.
    pub fn max_retries(&self) -> u32 {
        self.config.max_retries
    }

    /// Refresh visibility/ownership for a job that is still processing.
    /// This resets the idle timer so long-running jobs are not reclaimed while active.
    pub async fn refresh_visibility(
        &self,
        consumer_name: &str,
        message_id: &str,
    ) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // XCLAIM with min-idle=0 moves the message to this consumer and resets its idle time.
        // JUSTID avoids transferring the full payload and keeps this lightweight.
        let _res: redis::Value = redis::cmd("XCLAIM")
            .arg(&self.config.stream_name)
            .arg(&self.config.consumer_group)
            .arg(consumer_name)
            .arg(0) // min-idle-ms
            .arg(message_id)
            .arg("JUSTID")
            .query_async(&mut conn)
            .await?;

        Ok(())
    }
}
