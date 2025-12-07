//! Speaker detection module for podcast-style videos.
//!
//! This module analyzes audio to determine which person is speaking
//! in a two-person side-by-side video layout (podcast format).
//!
//! # Architecture
//!
//! Uses FFmpeg's audio analysis capabilities to detect:
//! 1. Audio volume levels over time
//! 2. Audio activity (voice activity detection approximation)
//! 3. Stereo channel balance (if available)
//!
//! For mono audio, uses heuristics based on motion analysis in each half.

use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

/// Result of speaker detection for a time segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveSpeaker {
    /// Left person is speaking
    Left,
    /// Right person is speaking
    Right,
    /// Both are speaking or unclear
    Both,
    /// No one is speaking (silence)
    None,
}

/// Speaker activity for a segment of time.
#[derive(Debug, Clone)]
pub struct SpeakerSegment {
    /// Start time in seconds
    pub start_time: f64,
    /// End time in seconds
    pub end_time: f64,
    /// Detected active speaker
    pub speaker: ActiveSpeaker,
    /// Confidence (0.0 - 1.0)
    pub confidence: f64,
}

/// Configuration for speaker detection.
#[derive(Debug, Clone)]
pub struct SpeakerDetectorConfig {
    /// Minimum segment duration for speaker detection (seconds)
    pub min_segment_duration: f64,
    /// Volume threshold for voice activity (0.0 - 1.0)
    pub volume_threshold: f64,
    /// Samples per second for audio analysis
    pub sample_rate: f64,
    /// Balance threshold to determine left vs right (stereo)
    pub balance_threshold: f64,
    /// Motion differential threshold for fallback detection
    pub motion_threshold: f64,
}

impl Default for SpeakerDetectorConfig {
    fn default() -> Self {
        Self {
            min_segment_duration: 0.5,
            volume_threshold: 0.1,
            sample_rate: 10.0,
            balance_threshold: 0.15,
            motion_threshold: 0.2,
        }
    }
}

/// Speaker detector using audio and video analysis.
pub struct SpeakerDetector {
    config: SpeakerDetectorConfig,
}

impl SpeakerDetector {
    /// Create a new speaker detector with default configuration.
    pub fn new() -> Self {
        Self {
            config: SpeakerDetectorConfig::default(),
        }
    }

    /// Create a new speaker detector with custom configuration.
    pub fn with_config(config: SpeakerDetectorConfig) -> Self {
        Self { config }
    }

    /// Analyze a video segment to detect speaker activity.
    ///
    /// Returns a list of segments with the active speaker for each.
    pub async fn detect_speakers<P: AsRef<Path>>(
        &self,
        video_path: P,
        duration: f64,
        width: u32,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let video_path = video_path.as_ref();

        info!("Analyzing speaker activity for {:?}", video_path);

        // Try stereo audio analysis first
        let stereo_result = self.analyze_stereo_audio(video_path, duration).await;

        if let Ok(segments) = stereo_result {
            if !segments.is_empty() {
                info!("Using stereo audio analysis: {} segments", segments.len());
                return Ok(segments);
            }
        }

        // Fallback to motion-based speaker detection
        info!("Falling back to motion-based speaker detection");
        self.analyze_motion_for_speakers(video_path, duration, width).await
    }

    /// Analyze stereo audio to detect left/right speaker activity.
    async fn analyze_stereo_audio<P: AsRef<Path>>(
        &self,
        video_path: P,
        duration: f64,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let video_path = video_path.as_ref();
        let num_samples = (duration * self.config.sample_rate).ceil() as usize;
        let sample_interval = 1.0 / self.config.sample_rate;

        // Use FFmpeg to extract audio levels for left and right channels
        // astats filter provides detailed audio statistics including RMS levels
        let filter = format!(
            "asplit[a][b];[a]pan=mono|c0=c0,astats=metadata=1:reset={:.4}[left];[b]pan=mono|c0=c1,astats=metadata=1:reset={:.4}[right];[left][right]amix=inputs=2[out]",
            sample_interval,
            sample_interval
        );

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i",
            video_path.to_str().unwrap_or(""),
            "-t",
            &format!("{:.3}", duration),
            "-af",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(
                format!("Failed to analyze stereo audio: {}", e),
                None,
                None,
            )
        })?;

        if !output.status.success() {
            // Stereo analysis failed - likely mono audio
            debug!("Stereo audio analysis failed, likely mono audio");
            return Ok(Vec::new());
        }

        // Parse audio levels from FFmpeg output
        let stderr = String::from_utf8_lossy(&output.stderr);
        self.parse_stereo_audio_levels(&stderr, duration, num_samples)
    }

    /// Parse stereo audio levels from FFmpeg astats output.
    fn parse_stereo_audio_levels(
        &self,
        output: &str,
        duration: f64,
        num_samples: usize,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let mut left_levels: Vec<f64> = Vec::new();
        let mut right_levels: Vec<f64> = Vec::new();

        // Parse RMS levels from astats metadata
        // Format: lavfi.astats.1.RMS_level=-XX.XX
        for line in output.lines() {
            if line.contains("RMS_level") || line.contains("rms_level") {
                if let Some(level_str) = line.split('=').nth(1) {
                    if let Ok(db_level) = level_str.trim().parse::<f64>() {
                        // Convert dB to linear (clamped to avoid inf/NaN)
                        let linear = if db_level <= -100.0 {
                            0.0
                        } else {
                            10f64.powf(db_level / 20.0)
                        };

                        // Assign to left or right based on channel number in the line
                        if line.contains(".0.") || line.contains("c0") {
                            left_levels.push(linear);
                        } else if line.contains(".1.") || line.contains("c1") {
                            right_levels.push(linear);
                        }
                    }
                }
            }
        }

        // If we didn't get enough samples, the audio might be mono
        if left_levels.len() < 3 || right_levels.len() < 3 {
            return Ok(Vec::new());
        }

        // Normalize sample counts
        let left_levels = Self::resample_to_count(&left_levels, num_samples);
        let right_levels = Self::resample_to_count(&right_levels, num_samples);

        // Generate speaker segments
        self.generate_segments_from_levels(&left_levels, &right_levels, duration)
    }

    /// Resample a vector to have exactly the target count.
    fn resample_to_count(values: &[f64], target_count: usize) -> Vec<f64> {
        if values.len() == target_count || values.is_empty() {
            return values.to_vec();
        }

        let mut resampled = Vec::with_capacity(target_count);
        let ratio = values.len() as f64 / target_count as f64;

        for i in 0..target_count {
            let src_idx = (i as f64 * ratio).floor() as usize;
            let src_idx = src_idx.min(values.len() - 1);
            resampled.push(values[src_idx]);
        }

        resampled
    }

    /// Generate speaker segments from left/right audio levels.
    fn generate_segments_from_levels(
        &self,
        left_levels: &[f64],
        right_levels: &[f64],
        duration: f64,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let num_samples = left_levels.len();
        let sample_interval = duration / num_samples as f64;

        let mut segments: Vec<SpeakerSegment> = Vec::new();
        let mut current_speaker = ActiveSpeaker::None;
        let mut segment_start = 0.0;

        for i in 0..num_samples {
            let left = left_levels[i];
            let right = right_levels[i];
            let time = i as f64 * sample_interval;

            // Determine speaker for this sample
            let speaker = self.determine_speaker(left, right);

            // Check for speaker change
            if speaker != current_speaker {
                // Close previous segment if it's long enough
                if time - segment_start >= self.config.min_segment_duration {
                    segments.push(SpeakerSegment {
                        start_time: segment_start,
                        end_time: time,
                        speaker: current_speaker,
                        confidence: self.compute_confidence(left, right),
                    });
                }
                current_speaker = speaker;
                segment_start = time;
            }
        }

        // Close final segment
        if duration - segment_start >= self.config.min_segment_duration {
            segments.push(SpeakerSegment {
                start_time: segment_start,
                end_time: duration,
                speaker: current_speaker,
                confidence: 0.7,
            });
        }

        Ok(segments)
    }

    /// Determine which speaker is active based on left/right levels.
    fn determine_speaker(&self, left: f64, right: f64) -> ActiveSpeaker {
        let total = left + right;

        // Check for silence
        if total < self.config.volume_threshold {
            return ActiveSpeaker::None;
        }

        // Compute balance (-1 = all left, +1 = all right)
        let balance = if total > 0.0 {
            (right - left) / total
        } else {
            0.0
        };

        if balance < -self.config.balance_threshold {
            ActiveSpeaker::Left
        } else if balance > self.config.balance_threshold {
            ActiveSpeaker::Right
        } else {
            ActiveSpeaker::Both
        }
    }

    /// Compute confidence based on audio levels.
    fn compute_confidence(&self, left: f64, right: f64) -> f64 {
        let total = left + right;
        if total < self.config.volume_threshold {
            return 0.5; // Low confidence for silence
        }

        // Higher confidence when there's clear asymmetry
        let diff = (left - right).abs();
        let balance_confidence = (diff / total).min(1.0);

        0.5 + balance_confidence * 0.5
    }

    /// Fallback: Analyze motion in left/right halves to detect speaker.
    ///
    /// Speaking typically causes more head/lip movement, so we can use
    /// motion as a proxy for voice activity.
    async fn analyze_motion_for_speakers<P: AsRef<Path>>(
        &self,
        video_path: P,
        duration: f64,
        width: u32,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let video_path = video_path.as_ref();
        let half_width = width / 2;

        // Analyze motion in left half
        let left_motion = self
            .analyze_region_motion(video_path, 0, 0, half_width, duration)
            .await?;

        // Analyze motion in right half
        let right_motion = self
            .analyze_region_motion(video_path, half_width, 0, half_width, duration)
            .await?;

        // Compare motion levels to determine speaker
        self.motion_to_speaker_segments(&left_motion, &right_motion, duration)
    }

    /// Analyze motion in a specific region of the video.
    async fn analyze_region_motion<P: AsRef<Path>>(
        &self,
        video_path: P,
        x: u32,
        y: u32,
        width: u32,
        duration: f64,
    ) -> MediaResult<Vec<f64>> {
        let video_path = video_path.as_ref();
        let num_samples = (duration * self.config.sample_rate).ceil() as usize;

        // Use FFmpeg to analyze motion in the cropped region
        let filter = format!(
            "crop={}:ih:{}:{},fps={},select='gt(scene,0)',showinfo",
            width, x, y, self.config.sample_rate
        );

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i",
            video_path.to_str().unwrap_or(""),
            "-t",
            &format!("{:.3}", duration),
            "-vf",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to analyze motion: {}", e), None, None)
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse scene change scores
        let mut motion_levels = Vec::new();

        for line in stderr.lines() {
            if line.contains("scene:") {
                if let Some(score_part) = line.split("scene:").nth(1) {
                    if let Some(score_str) = score_part.split_whitespace().next() {
                        if let Ok(score) = score_str.parse::<f64>() {
                            motion_levels.push(score);
                        }
                    }
                }
            }
        }

        // Fill to expected sample count
        while motion_levels.len() < num_samples {
            motion_levels.push(0.0);
        }

        Ok(motion_levels.into_iter().take(num_samples).collect())
    }

    /// Convert motion analysis to speaker segments.
    fn motion_to_speaker_segments(
        &self,
        left_motion: &[f64],
        right_motion: &[f64],
        duration: f64,
    ) -> MediaResult<Vec<SpeakerSegment>> {
        let num_samples = left_motion.len().min(right_motion.len());
        if num_samples == 0 {
            return Ok(vec![SpeakerSegment {
                start_time: 0.0,
                end_time: duration,
                speaker: ActiveSpeaker::Left, // Default to left (host)
                confidence: 0.5,
            }]);
        }

        let sample_interval = duration / num_samples as f64;
        let mut segments: Vec<SpeakerSegment> = Vec::new();
        let mut current_speaker = ActiveSpeaker::None;
        let mut segment_start = 0.0;

        for i in 0..num_samples {
            let left = left_motion[i];
            let right = right_motion[i];
            let time = i as f64 * sample_interval;

            // Determine which side has more motion
            let diff = right - left;
            let speaker = if diff.abs() < self.config.motion_threshold {
                // Similar motion - keep current or default to left
                if current_speaker == ActiveSpeaker::None {
                    ActiveSpeaker::Left
                } else {
                    current_speaker
                }
            } else if diff > 0.0 {
                ActiveSpeaker::Right
            } else {
                ActiveSpeaker::Left
            };

            // Check for speaker change
            if speaker != current_speaker && current_speaker != ActiveSpeaker::None {
                // Close previous segment
                if time - segment_start >= self.config.min_segment_duration {
                    segments.push(SpeakerSegment {
                        start_time: segment_start,
                        end_time: time,
                        speaker: current_speaker,
                        confidence: 0.6,
                    });
                }
                segment_start = time;
            }
            current_speaker = speaker;
        }

        // Close final segment
        if segments.is_empty() || duration - segment_start >= self.config.min_segment_duration {
            segments.push(SpeakerSegment {
                start_time: if segments.is_empty() {
                    0.0
                } else {
                    segment_start
                },
                end_time: duration,
                speaker: if current_speaker == ActiveSpeaker::None {
                    ActiveSpeaker::Left
                } else {
                    current_speaker
                },
                confidence: 0.6,
            });
        }

        // If no segments, default to left speaker
        if segments.is_empty() {
            segments.push(SpeakerSegment {
                start_time: 0.0,
                end_time: duration,
                speaker: ActiveSpeaker::Left,
                confidence: 0.5,
            });
        }

        Ok(segments)
    }

    /// Get the speaker at a specific time.
    pub fn speaker_at_time(&self, segments: &[SpeakerSegment], time: f64) -> ActiveSpeaker {
        for segment in segments {
            if time >= segment.start_time && time < segment.end_time {
                return segment.speaker;
            }
        }
        ActiveSpeaker::Left // Default
    }
}

impl Default for SpeakerDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_speaker_left() {
        let detector = SpeakerDetector::new();

        // Strong left activity
        let speaker = detector.determine_speaker(0.8, 0.1);
        assert_eq!(speaker, ActiveSpeaker::Left);
    }

    #[test]
    fn test_determine_speaker_right() {
        let detector = SpeakerDetector::new();

        // Strong right activity
        let speaker = detector.determine_speaker(0.1, 0.8);
        assert_eq!(speaker, ActiveSpeaker::Right);
    }

    #[test]
    fn test_determine_speaker_both() {
        let detector = SpeakerDetector::new();

        // Similar activity both sides
        let speaker = detector.determine_speaker(0.5, 0.5);
        assert_eq!(speaker, ActiveSpeaker::Both);
    }

    #[test]
    fn test_determine_speaker_silence() {
        let detector = SpeakerDetector::new();

        // Low activity
        let speaker = detector.determine_speaker(0.01, 0.02);
        assert_eq!(speaker, ActiveSpeaker::None);
    }

    #[test]
    fn test_resample() {
        let values = vec![1.0, 2.0, 3.0, 4.0];

        // Upsample
        let upsampled = SpeakerDetector::resample_to_count(&values, 8);
        assert_eq!(upsampled.len(), 8);

        // Downsample
        let downsampled = SpeakerDetector::resample_to_count(&values, 2);
        assert_eq!(downsampled.len(), 2);
    }
}
