//! Job executor.

use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use vclip_queue::{JobQueue, ProcessVideoJob, ProgressChannel, QueueJob, ReprocessScenesJob};

use crate::config::WorkerConfig;
use crate::error::WorkerResult;
use crate::processor::{process_video, reprocess_scenes, ProcessingContext};

/// Job executor that processes jobs from the queue.
pub struct JobExecutor {
    config: WorkerConfig,
    queue: JobQueue,
    job_semaphore: Arc<Semaphore>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl JobExecutor {
    /// Create a new job executor.
    pub fn new(config: WorkerConfig, queue: JobQueue) -> Self {
        let job_semaphore = Arc::new(Semaphore::new(config.max_concurrent_jobs));
        let (shutdown, _) = tokio::sync::watch::channel(false);

        Self {
            config,
            queue,
            job_semaphore,
            shutdown,
        }
    }

    /// Start the executor.
    pub async fn run(&self) -> WorkerResult<()> {
        info!("Starting job executor with {} max concurrent jobs", self.config.max_concurrent_jobs);

        // Initialize queue
        self.queue.init().await?;

        // Create processing context
        let ctx = Arc::new(ProcessingContext::new(self.config.clone()).await?);

        let mut shutdown_rx = self.shutdown.subscribe();

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Shutdown signal received, stopping executor");
                        break;
                    }
                }
                // In a real implementation, we'd use Apalis to consume jobs
                // For now, this is a placeholder that would need the Apalis integration
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    // Poll for jobs
                }
            }
        }

        info!("Job executor stopped");
        Ok(())
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
