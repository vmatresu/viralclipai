//! Progress events via Redis Pub/Sub.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::debug;

use vclip_models::{ClipProcessingStep, JobId, WsMessage};

use crate::error::QueueResult;

/// Progress event published to Redis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Job ID
    pub job_id: JobId,
    /// WebSocket message
    pub message: WsMessage,
}

/// Channel for publishing/subscribing to progress events.
pub struct ProgressChannel {
    client: redis::Client,
}

impl ProgressChannel {
    /// Create a new progress channel.
    pub fn new(redis_url: &str) -> QueueResult<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }

    /// Get the channel name for a job.
    pub fn channel_name(job_id: &JobId) -> String {
        format!("progress:{}", job_id)
    }

    /// Publish a progress event.
    pub async fn publish(&self, event: &ProgressEvent) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let channel = Self::channel_name(&event.job_id);
        let payload = serde_json::to_string(event)?;

        debug!("Publishing progress event to {}", channel);
        conn.publish::<_, _, ()>(channel, payload).await?;

        Ok(())
    }

    /// Publish a log message.
    pub async fn log(&self, job_id: &JobId, message: impl Into<String>) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::log(message),
        })
        .await
    }

    /// Publish a progress update.
    pub async fn progress(&self, job_id: &JobId, value: u8) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::progress(value),
        })
        .await
    }

    /// Publish a clip uploaded notification.
    pub async fn clip_uploaded(
        &self,
        job_id: &JobId,
        video_id: &str,
        clip_count: u32,
        total_clips: u32,
    ) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::clip_uploaded(video_id, clip_count, total_clips),
        })
        .await
    }

    /// Publish done message.
    pub async fn done(&self, job_id: &JobId, video_id: &str) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::done(video_id),
        })
        .await
    }

    /// Publish error message.
    pub async fn error(&self, job_id: &JobId, message: impl Into<String>) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::error(message),
        })
        .await
    }

    /// Publish clip progress message.
    pub async fn clip_progress(
        &self,
        job_id: &JobId,
        scene_id: u32,
        style: &str,
        step: ClipProcessingStep,
        details: Option<String>,
    ) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::clip_progress(scene_id, style, step, details),
        })
        .await
    }

    /// Publish scene started message.
    pub async fn scene_started(
        &self,
        job_id: &JobId,
        scene_id: u32,
        scene_title: &str,
        style_count: u32,
        start_sec: f64,
        duration_sec: f64,
    ) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::scene_started(scene_id, scene_title, style_count, start_sec, duration_sec),
        })
        .await
    }

    /// Publish scene completed message.
    pub async fn scene_completed(
        &self,
        job_id: &JobId,
        scene_id: u32,
        clips_completed: u32,
        clips_failed: u32,
    ) -> QueueResult<()> {
        self.publish(&ProgressEvent {
            job_id: job_id.clone(),
            message: WsMessage::scene_completed(scene_id, clips_completed, clips_failed),
        })
        .await
    }

    /// Subscribe to progress events for a job.
    /// Returns a pinned stream that can be polled with `.next()`.
    pub async fn subscribe(
        &self,
        job_id: &JobId,
    ) -> QueueResult<std::pin::Pin<Box<dyn futures_util::Stream<Item = ProgressEvent> + Send>>> {
        use futures_util::StreamExt;

        let mut pubsub = self.client.get_async_pubsub().await?;
        let channel = Self::channel_name(job_id);

        pubsub.subscribe(&channel).await?;

        let stream = pubsub.into_on_message().filter_map(|msg| async move {
            let payload: String = msg.get_payload().ok()?;
            serde_json::from_str(&payload).ok()
        });

        Ok(Box::pin(stream))
    }
}
