//! State machine for converting VAD output to Keep/Cut segments.
//!
//! The segmenter processes a stream of speech probabilities and produces
//! a timeline of segments that should be kept or cut from the video.
//!
//! # State Machine
//!
//! ```text
//!                    speech_prob >= threshold
//!     ┌─────────────────────────────────────────────────┐
//!     │                                                 │
//!     ▼                                                 │
//! ┌─────────┐                                     ┌─────────┐
//! │InSpeech │─────────────────────────────────────│InSilence│
//! └─────────┘     speech_prob < threshold         └─────────┘
//!     │                                                 │
//!     │           silence_duration > min_silence        │
//!     └──────────────── Mark as CUT ────────────────────┘
//! ```

use super::config::SilenceRemovalConfig;

/// Label indicating whether a segment should be kept or cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentLabel {
    /// Keep this segment in the output.
    Keep,
    /// Cut (remove) this segment from the output.
    Cut,
}

/// A time segment with a Keep or Cut label.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Start time in milliseconds.
    pub start_ms: u64,
    /// End time in milliseconds.
    pub end_ms: u64,
    /// Whether to keep or cut this segment.
    pub label: SegmentLabel,
}

impl Segment {
    /// Duration of this segment in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    /// Duration of this segment in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_ms() as f64 / 1000.0
    }
}

/// Internal state for the segmenter state machine.
enum State {
    /// Currently processing speech content.
    InSpeech,
    /// Currently processing silence, tracking when it started.
    InSilence { silence_start_ms: u64 },
}

/// Converts a stream of VAD probabilities into Keep/Cut segments.
pub struct SilenceRemover {
    config: SilenceRemovalConfig,
    state: State,
    segments: Vec<Segment>,
    current_segment_start: u64,
}

impl SilenceRemover {
    /// Create a new SilenceRemover with the given configuration.
    pub fn new(config: SilenceRemovalConfig) -> Self {
        Self {
            config,
            // Assume silence at start until proven otherwise to catch initial dead air
            state: State::InSilence {
                silence_start_ms: 0,
            },
            segments: Vec::new(),
            current_segment_start: 0,
        }
    }

    /// Process a single frame of VAD output.
    ///
    /// # Arguments
    /// - `speech_prob`: Speech probability from VAD (0.0 to 1.0)
    /// - `timestamp_ms`: Current timestamp in milliseconds
    pub fn ingest_frame(&mut self, speech_prob: f32, timestamp_ms: u64) {
        let is_speech = speech_prob >= self.config.vad_threshold;

        match (&self.state, is_speech) {
            // Case: We were in silence, but now someone spoke
            (State::InSilence { silence_start_ms }, true) => {
                let silence_duration = timestamp_ms.saturating_sub(*silence_start_ms);

                if silence_duration > self.config.min_silence_ms {
                    // It was a long silence. CUT IT.
                    // Calculate where the cut should end (padding before speech starts)
                    let cut_end = timestamp_ms.saturating_sub(self.config.pre_speech_padding_ms);

                    // Only add segments if they actually advance time
                    if cut_end > self.current_segment_start {
                        // Push the KEEP segment leading up to this silence (if any content before)
                        if *silence_start_ms > self.current_segment_start {
                            self.segments.push(Segment {
                                start_ms: self.current_segment_start,
                                end_ms: *silence_start_ms,
                                label: SegmentLabel::Keep,
                            });
                        }

                        // Push the CUT segment for the silence
                        self.segments.push(Segment {
                            start_ms: *silence_start_ms,
                            end_ms: cut_end,
                            label: SegmentLabel::Cut,
                        });

                        self.current_segment_start = cut_end;
                    }
                }

                // Transition state
                self.state = State::InSpeech;
            }

            // Case: We were speaking, now it's silent
            (State::InSpeech, false) => {
                self.state = State::InSilence {
                    silence_start_ms: timestamp_ms,
                };
            }

            // Continue in current state
            _ => {}
        }
    }

    /// Finalize processing and return all segments.
    ///
    /// This must be called after all frames have been ingested to handle
    /// the final segment properly.
    ///
    /// # Arguments
    /// - `total_duration_ms`: Total duration of the input in milliseconds
    pub fn finalize(mut self, total_duration_ms: u64) -> Vec<Segment> {
        // Handle the final state
        match self.state {
            State::InSilence { silence_start_ms } => {
                let silence_duration = total_duration_ms.saturating_sub(silence_start_ms);

                if silence_duration > self.config.min_silence_ms {
                    // The video ends with silence.
                    // Push the final KEEP segment up to the start of silence (+ padding)
                    let cut_start = silence_start_ms + self.config.post_speech_padding_ms;

                    if cut_start < total_duration_ms && self.current_segment_start < cut_start {
                        self.segments.push(Segment {
                            start_ms: self.current_segment_start,
                            end_ms: cut_start,
                            label: SegmentLabel::Keep,
                        });
                        self.segments.push(Segment {
                            start_ms: cut_start,
                            end_ms: total_duration_ms,
                            label: SegmentLabel::Cut,
                        });
                    } else if self.current_segment_start < total_duration_ms {
                        // Padding pushed us past the end, just keep everything
                        self.segments.push(Segment {
                            start_ms: self.current_segment_start,
                            end_ms: total_duration_ms,
                            label: SegmentLabel::Keep,
                        });
                    }
                } else if self.current_segment_start < total_duration_ms {
                    // Silence wasn't long enough, keep it all
                    self.segments.push(Segment {
                        start_ms: self.current_segment_start,
                        end_ms: total_duration_ms,
                        label: SegmentLabel::Keep,
                    });
                }
            }
            State::InSpeech => {
                // Ended while speaking, keep the rest
                if self.current_segment_start < total_duration_ms {
                    self.segments.push(Segment {
                        start_ms: self.current_segment_start,
                        end_ms: total_duration_ms,
                        label: SegmentLabel::Keep,
                    });
                }
            }
        }

        self.segments
    }

    /// Get the number of segments processed so far.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

/// Calculate statistics about the segments.
pub fn compute_segment_stats(segments: &[Segment]) -> SegmentStats {
    let mut total_keep_ms = 0u64;
    let mut total_cut_ms = 0u64;
    let mut keep_count = 0usize;
    let mut cut_count = 0usize;

    for segment in segments {
        let duration = segment.duration_ms();
        match segment.label {
            SegmentLabel::Keep => {
                total_keep_ms += duration;
                keep_count += 1;
            }
            SegmentLabel::Cut => {
                total_cut_ms += duration;
                cut_count += 1;
            }
        }
    }

    let total_ms = total_keep_ms + total_cut_ms;
    let keep_ratio = if total_ms > 0 {
        total_keep_ms as f64 / total_ms as f64
    } else {
        1.0
    };

    SegmentStats {
        total_keep_ms,
        total_cut_ms,
        keep_count,
        cut_count,
        keep_ratio,
    }
}

/// Statistics about Keep/Cut segments.
#[derive(Debug, Clone)]
pub struct SegmentStats {
    /// Total duration of Keep segments in milliseconds.
    pub total_keep_ms: u64,
    /// Total duration of Cut segments in milliseconds.
    pub total_cut_ms: u64,
    /// Number of Keep segments.
    pub keep_count: usize,
    /// Number of Cut segments.
    pub cut_count: usize,
    /// Ratio of kept content (0.0 to 1.0).
    pub keep_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> SilenceRemovalConfig {
        SilenceRemovalConfig {
            vad_threshold: 0.5,
            min_silence_ms: 1000,
            pre_speech_padding_ms: 200,
            post_speech_padding_ms: 200,
            min_keep_ratio: 0.1,
            max_inline_segments: 100,
        }
    }

    #[test]
    fn test_all_speech() {
        let config = make_config();
        let mut remover = SilenceRemover::new(config);

        // Continuous speech
        for i in 0..100 {
            remover.ingest_frame(0.8, i * 30);
        }

        let segments = remover.finalize(3000);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].label, SegmentLabel::Keep);
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].end_ms, 3000);
    }

    #[test]
    fn test_all_silence() {
        let config = make_config();
        let mut remover = SilenceRemover::new(config);

        // Continuous silence for 3 seconds
        for i in 0..100 {
            remover.ingest_frame(0.1, i * 30);
        }

        let segments = remover.finalize(3000);

        // Should have one Cut segment (silence > min_silence_ms)
        let cut_segments: Vec<_> = segments
            .iter()
            .filter(|s| s.label == SegmentLabel::Cut)
            .collect();
        assert!(
            !cut_segments.is_empty(),
            "Should have at least one cut segment"
        );
    }

    #[test]
    fn test_speech_silence_speech() {
        let config = make_config();
        let mut remover = SilenceRemover::new(config);

        // Speech (0-1000ms)
        for i in 0..33 {
            remover.ingest_frame(0.8, i * 30);
        }

        // Silence (1000-3000ms) - 2 seconds
        for i in 33..100 {
            remover.ingest_frame(0.1, i * 30);
        }

        // Speech (3000-4000ms)
        for i in 100..133 {
            remover.ingest_frame(0.8, i * 30);
        }

        let segments = remover.finalize(4000);

        // Should have: Keep, Cut, Keep pattern
        let labels: Vec<_> = segments.iter().map(|s| s.label).collect();
        assert!(
            labels.iter().any(|l| *l == SegmentLabel::Cut),
            "Should have at least one Cut segment"
        );
    }

    #[test]
    fn test_short_silence_not_cut() {
        let config = make_config(); // min_silence_ms = 1000
        let mut remover = SilenceRemover::new(config);

        // Speech (0-500ms)
        for i in 0..17 {
            remover.ingest_frame(0.8, i * 30);
        }

        // Short silence (500-900ms) - only 400ms, less than threshold
        for i in 17..30 {
            remover.ingest_frame(0.1, i * 30);
        }

        // Speech (900-1500ms)
        for i in 30..50 {
            remover.ingest_frame(0.8, i * 30);
        }

        let segments = remover.finalize(1500);

        // Should be all Keep (short silence not cut)
        let cut_count = segments
            .iter()
            .filter(|s| s.label == SegmentLabel::Cut)
            .count();
        assert_eq!(cut_count, 0, "Short silence should not be cut");
    }

    #[test]
    fn test_segment_stats() {
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 1000,
                label: SegmentLabel::Keep,
            },
            Segment {
                start_ms: 1000,
                end_ms: 2000,
                label: SegmentLabel::Cut,
            },
            Segment {
                start_ms: 2000,
                end_ms: 3000,
                label: SegmentLabel::Keep,
            },
        ];

        let stats = compute_segment_stats(&segments);
        assert_eq!(stats.total_keep_ms, 2000);
        assert_eq!(stats.total_cut_ms, 1000);
        assert_eq!(stats.keep_count, 2);
        assert_eq!(stats.cut_count, 1);
        assert!((stats.keep_ratio - 0.667).abs() < 0.01);
    }
}
