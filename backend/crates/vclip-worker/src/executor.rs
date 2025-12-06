//! Job executor.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tracing::{error, info, warn, debug};
use uuid::Uuid;

use vclip_queue::{JobQueue, QueueJob};

use crate::config::WorkerConfig;
use crate::error::{WorkerError, WorkerResult};
use crate::processor::{process_video, reprocess_scenes, ProcessingContext};

/// Job executor that processes jobs from the queue.
pub struct JobExecutor {
    config: WorkerConfig,
    queue: Arc<JobQueue>,
    job_semaphore: Arc<Semaphore>,
    shutdown: tokio::sync::watch::Sender<bool>,
    consumer_name: String,
}

impl JobExecutor {
    /// Create a new job executor.
    pub fn new(config: WorkerConfig, queue: JobQueue) -> Self {
        let job_semaphore = Arc::new(Semaphore::new(config.max_concurrent_jobs));
        let (shutdown, _) = tokio::sync::watch::channel(false);
        let consumer_name = format!("worker-{}", Uuid::new_v4());

        Self {
            config,
            queue: Arc::new(queue),
            job_semaphore,
            shutdown,
            consumer_name,
        }
    }

    /// Start the executor.
    pub async fn run(&self) -> WorkerResult<()> {
        info!(
            "Starting job executor '{}' with {} max concurrent jobs",
            self.consumer_name, self.config.max_concurrent_jobs
        );

        // Initialize queue
        self.queue.init().await?;

        // Create processing context
        let ctx = Arc::new(ProcessingContext::new(self.config.clone()).await?);

        let mut shutdown_rx = self.shutdown.subscribe();

        // Spawn a task to claim pending jobs periodically
        let queue_clone = Arc::clone(&self.queue);
        let consumer_name = self.consumer_name.clone();
        let ctx_clone = Arc::clone(&ctx);
        let semaphore_clone = Arc::clone(&self.job_semaphore);
        let mut shutdown_rx_claim = self.shutdown.subscribe();

        let claim_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = shutdown_rx_claim.changed() => {
                        if *shutdown_rx_claim.borrow() {
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        // Claim jobs that have been pending for more than 5 minutes
                        match queue_clone.claim_pending(&consumer_name, 300_000, 5).await {
                            Ok(jobs) if !jobs.is_empty() => {
                                info!("Claimed {} pending jobs", jobs.len());
                                for (message_id, job) in jobs {
                                    let ctx = Arc::clone(&ctx_clone);
                                    let queue = Arc::clone(&queue_clone);
                                    let permit = semaphore_clone.clone().acquire_owned().await;
                                    if permit.is_err() {
                                        break;
                                    }
                                    let permit = permit.unwrap();

                                    tokio::spawn(async move {
                                        let _permit = permit;
                                        Self::execute_job(ctx, queue, message_id, job).await;
                                    });
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!("Failed to claim pending jobs: {}", e);
                            }
                        }
                    }
                }
            }
        });

        // Main job consumption loop
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Shutdown signal received, stopping executor");
                        break;
                    }
                }
                result = self.consume_jobs(&ctx) => {
                    if let Err(e) = result {
                        error!("Error consuming jobs: {}", e);
                        // Back off on error
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }

        // Wait for claim task to finish
        claim_task.abort();

        // Wait for in-flight jobs to complete
        info!("Waiting for in-flight jobs to complete...");
        let _ = tokio::time::timeout(
            Duration::from_secs(60),
            self.wait_for_jobs(),
        ).await;

        info!("Job executor stopped");
        Ok(())
    }

    /// Consume and process jobs from the queue.
    async fn consume_jobs(&self, ctx: &Arc<ProcessingContext>) -> WorkerResult<()> {
        // Acquire semaphore permit before consuming
        let available = self.job_semaphore.available_permits();
        if available == 0 {
            // All slots busy, wait a bit
            tokio::time::sleep(Duration::from_millis(100)).await;
            return Ok(());
        }

        // Consume up to available slots
        let jobs = self.queue.consume(
            &self.consumer_name,
            1000, // Block for 1 second
            available.min(5), // Max 5 jobs at a time
        ).await?;

        if jobs.is_empty() {
            return Ok(());
        }

        debug!("Consumed {} jobs from queue", jobs.len());

        for (message_id, job) in jobs {
            let ctx = Arc::clone(ctx);
            let queue = Arc::clone(&self.queue);
            let permit = self.job_semaphore.clone().acquire_owned().await
                .map_err(|_| WorkerError::job_failed("Semaphore closed"))?;

            tokio::spawn(async move {
                let _permit = permit;
                Self::execute_job(ctx, queue, message_id, job).await;
            });
        }

        Ok(())
    }

    /// Execute a single job with retry and DLQ handling.
    async fn execute_job(
        ctx: Arc<ProcessingContext>,
        queue: Arc<JobQueue>,
        message_id: String,
        job: QueueJob,
    ) {
        let job_id = job.job_id().to_string();
        info!("Executing job {}", job_id);

        let result = Self::process_job(Arc::clone(&ctx), job.clone()).await;

        match result {
            Ok(()) => {
                info!("Job {} completed successfully", job_id);
                if let Err(e) = queue.ack(&message_id).await {
                    error!("Failed to ack job {}: {}", job_id, e);
                }
                // Clear dedup key so the same job can be reprocessed later
                if let Err(e) = queue.clear_dedup(&job).await {
                    warn!("Failed to clear dedup key for job {}: {}", job_id, e);
                }
            }
            Err(e) => {
                error!("Job {} failed: {}", job_id, e);

                // Check retry count
                let retry_count = queue.increment_retry(&message_id).await.unwrap_or(999);
                let max_retries = queue.max_retries();

                if retry_count >= max_retries {
                    warn!("Job {} exceeded max retries ({}), moving to DLQ", job_id, max_retries);
                    if let Err(dlq_err) = queue.dlq(&message_id, &job, &e.to_string()).await {
                        error!("Failed to move job {} to DLQ: {}", job_id, dlq_err);
                    }
                    // Clear dedup key so the job can be retried manually later
                    if let Err(e) = queue.clear_dedup(&job).await {
                        warn!("Failed to clear dedup key for job {}: {}", job_id, e);
                    }

                    // Emit error to progress channel
                    ctx.progress.error(job.job_id(), format!("Job failed after {} retries: {}", max_retries, e)).await.ok();
                } else {
                    info!("Job {} will be retried (attempt {}/{})", job_id, retry_count, max_retries);
                    // Job will be redelivered after visibility timeout
                }
            }
        }
    }

    /// Wait for all in-flight jobs to complete.
    async fn wait_for_jobs(&self) {
        loop {
            let available = self.job_semaphore.available_permits();
            if available == self.config.max_concurrent_jobs {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Process a single job.
    async fn process_job(ctx: Arc<ProcessingContext>, job: QueueJob) -> WorkerResult<()> {
        match job {
            QueueJob::ProcessVideo(j) => process_video(&ctx, &j).await,
            QueueJob::ReprocessScenes(j) => reprocess_scenes(&ctx, &j).await,
        }
    }
}
