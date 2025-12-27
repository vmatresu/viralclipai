//! Progress reporting for media processing operations.
//!
//! This module provides a callback-based progress reporting system that allows
//! media processing functions to emit detailed progress events without being
//! tightly coupled to the transport mechanism (WebSocket, logging, etc.).

use std::sync::Arc;
use tokio::sync::mpsc;

/// Progress event emitted during clip processing.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Extracting video segment
    ExtractingSegment { start_sec: f64, duration_sec: f64 },

    /// Detecting faces in frames
    DetectingFaces { total_frames: u32 },

    /// Face detection complete
    FaceDetectionComplete {
        detections: u32,
        frames_with_faces: u32,
    },

    /// Computing camera path
    ComputingCameraPath,

    /// Camera path complete
    CameraPathComplete { keyframes: u32 },

    /// Computing crop windows
    ComputingCropWindows,

    /// Rendering output
    Rendering { style: String },

    /// Render complete
    RenderComplete,

    /// Processing complete
    Complete,

    /// Processing failed
    Failed { error: String },
}

/// Progress callback type.
///
/// This is a function that receives progress events and can forward them
/// to the appropriate destination (WebSocket, logging, metrics, etc.).
pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

/// Progress sender for async contexts.
///
/// Uses a bounded channel to avoid blocking the processing thread.
#[derive(Clone)]
pub struct ProgressSender {
    tx: mpsc::Sender<ProgressEvent>,
    scene_id: u32,
    style: String,
}

impl ProgressSender {
    /// Create a new progress sender.
    pub fn new(tx: mpsc::Sender<ProgressEvent>, scene_id: u32, style: impl Into<String>) -> Self {
        Self {
            tx,
            scene_id,
            style: style.into(),
        }
    }

    /// Get the scene ID.
    pub fn scene_id(&self) -> u32 {
        self.scene_id
    }

    /// Get the style.
    pub fn style(&self) -> &str {
        &self.style
    }

    /// Send a progress event (non-blocking).
    pub fn send(&self, event: ProgressEvent) {
        // Use try_send to avoid blocking; drop events if channel is full
        let _ = self.tx.try_send(event);
    }

    /// Send extracting segment event.
    pub fn extracting_segment(&self, start_sec: f64, duration_sec: f64) {
        self.send(ProgressEvent::ExtractingSegment {
            start_sec,
            duration_sec,
        });
    }

    /// Send detecting faces event.
    pub fn detecting_faces(&self, total_frames: u32) {
        self.send(ProgressEvent::DetectingFaces { total_frames });
    }

    /// Send face detection complete event.
    pub fn face_detection_complete(&self, detections: u32, frames_with_faces: u32) {
        self.send(ProgressEvent::FaceDetectionComplete {
            detections,
            frames_with_faces,
        });
    }

    /// Send computing camera path event.
    pub fn computing_camera_path(&self) {
        self.send(ProgressEvent::ComputingCameraPath);
    }

    /// Send camera path complete event.
    pub fn camera_path_complete(&self, keyframes: u32) {
        self.send(ProgressEvent::CameraPathComplete { keyframes });
    }

    /// Send computing crop windows event.
    pub fn computing_crop_windows(&self) {
        self.send(ProgressEvent::ComputingCropWindows);
    }

    /// Send rendering event.
    pub fn rendering(&self) {
        self.send(ProgressEvent::Rendering {
            style: self.style.clone(),
        });
    }

    /// Send render complete event.
    pub fn render_complete(&self) {
        self.send(ProgressEvent::RenderComplete);
    }

    /// Send complete event.
    pub fn complete(&self) {
        self.send(ProgressEvent::Complete);
    }

    /// Send failed event.
    pub fn failed(&self, error: impl Into<String>) {
        self.send(ProgressEvent::Failed {
            error: error.into(),
        });
    }
}

/// Progress receiver for collecting events.
pub struct ProgressReceiver {
    rx: mpsc::Receiver<ProgressEvent>,
}

impl ProgressReceiver {
    /// Receive the next progress event.
    pub async fn recv(&mut self) -> Option<ProgressEvent> {
        self.rx.recv().await
    }

    /// Try to receive a progress event without blocking.
    pub fn try_recv(&mut self) -> Option<ProgressEvent> {
        self.rx.try_recv().ok()
    }
}

/// Create a progress channel pair.
///
/// Returns a sender that can be cloned and passed to processing functions,
/// and a receiver for collecting the events.
pub fn channel(scene_id: u32, style: impl Into<String>) -> (ProgressSender, ProgressReceiver) {
    let (tx, rx) = mpsc::channel(32);
    (
        ProgressSender::new(tx, scene_id, style),
        ProgressReceiver { rx },
    )
}

/// A no-op progress sender for when progress reporting is not needed.
pub fn noop_sender(scene_id: u32, style: impl Into<String>) -> ProgressSender {
    let (tx, _rx) = mpsc::channel(1);
    ProgressSender::new(tx, scene_id, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_progress_channel() {
        let (sender, mut receiver) = channel(1, "intelligent");

        sender.extracting_segment(10.0, 30.0);
        sender.detecting_faces(100);
        sender.complete();

        let event1 = receiver.recv().await.unwrap();
        assert!(matches!(event1, ProgressEvent::ExtractingSegment { .. }));

        let event2 = receiver.recv().await.unwrap();
        assert!(matches!(
            event2,
            ProgressEvent::DetectingFaces { total_frames: 100 }
        ));

        let event3 = receiver.recv().await.unwrap();
        assert!(matches!(event3, ProgressEvent::Complete));
    }

    #[test]
    fn test_noop_sender() {
        let sender = noop_sender(1, "split");
        // Should not panic even though receiver is dropped
        sender.extracting_segment(0.0, 10.0);
        sender.complete();
    }
}
