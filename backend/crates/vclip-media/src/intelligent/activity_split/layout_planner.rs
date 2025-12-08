//! Layout planner for Smart Split (Activity).
//!
//! Decides when to show a single full-frame primary speaker versus a
//! two-panel vertical split with primary/secondary assignments.

use std::collections::HashMap;

use super::TimelineFrame;
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::activity_scorer::TemporalActivityTracker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Full { primary: u32 },
    Split { primary: u32, secondary: u32 },
}

#[derive(Debug, Clone)]
pub struct LayoutSpan {
    pub start: f64,
    pub end: f64,
    pub layout: LayoutMode,
}

pub(crate) struct LayoutPlanner {
    config: IntelligentCropConfig,
    min_secondary_ratio: f64,
    layout_hold: f64,
    min_active_score: f64,
    secondary_switch_margin: f64,
}

impl LayoutPlanner {
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self {
            config,
            min_secondary_ratio: 0.45,
            layout_hold: 0.6,
            min_active_score: 0.05,
            secondary_switch_margin: 0.08,
        }
    }

    pub fn plan(&self, frames: &[TimelineFrame], duration: f64) -> MediaResult<Vec<LayoutSpan>> {
        if frames.is_empty() {
            return Err(MediaError::InvalidVideo(
                "Smart Split (Activity) requires at least one analyzed frame".to_string(),
            ));
        }

        let mut tracker = TemporalActivityTracker::new(self.config_to_activity_cfg());
        let mut smoothed_scores: HashMap<u32, f64> = HashMap::new();

        let mut primary: Option<u32> = None;
        let mut primary_since = frames[0].time;
        let mut secondary: Option<u32> = None;
        let mut secondary_since = frames[0].time;

        let mut current_layout: Option<LayoutMode> = None;
        let mut layout_since = frames[0].time;
        let mut spans: Vec<LayoutSpan> = Vec::new();
        let mut pending_layout: Option<(LayoutMode, f64)> = None;

        for frame in frames {
            if frame.raw_activity.is_empty() {
                continue;
            }

            // Update tracker with raw scores
            for (track_id, raw) in &frame.raw_activity {
                tracker.update_activity(*track_id, *raw, 0.0, frame.time);
            }

            smoothed_scores.clear();
            for (track_id, _) in &frame.raw_activity {
                let score = tracker.get_final_activity(*track_id, frame.time);
                smoothed_scores.insert(*track_id, score);
            }

            let (best_track, best_score) = best_track(&smoothed_scores);
            if best_score < self.min_active_score {
                continue;
            }

            // Primary hysteresis
            if let Some(current_primary) = primary {
                let current_score = *smoothed_scores.get(&current_primary).unwrap_or(&0.0);
                let should_switch = best_track != current_primary
                    && best_score > current_score + self.config.switch_margin
                    && frame.time - primary_since >= self.config.min_switch_duration;

                if should_switch || !smoothed_scores.contains_key(&current_primary) {
                    primary = Some(best_track);
                    primary_since = frame.time;
                }
            } else {
                primary = Some(best_track);
                primary_since = frame.time;
            }

            // Secondary selection (best non-primary)
            let (secondary_candidate, secondary_score_candidate) =
                second_best_track(&smoothed_scores, primary);

            if let Some(sec_id) = secondary_candidate {
                let should_take = match secondary {
                    None => true,
                    Some(current_sec) => {
                        let current_score = *smoothed_scores.get(&current_sec).unwrap_or(&0.0);
                        (sec_id != current_sec
                            && secondary_score_candidate
                                > current_score + self.secondary_switch_margin)
                            || !smoothed_scores.contains_key(&current_sec)
                    }
                };

                if should_take {
                    secondary = Some(sec_id);
                    secondary_since = frame.time;
                }
            }

            let secondary_score = secondary
                .and_then(|sec| smoothed_scores.get(&sec).copied())
                .unwrap_or(0.0);

            // Decide desired layout
            let desired_layout = match (primary, secondary) {
                (Some(p), Some(s))
                    if secondary_score >= best_score * self.min_secondary_ratio
                        && frame.time - secondary_since >= self.layout_hold =>
                {
                    LayoutMode::Split { primary: p, secondary: s }
                }
                (Some(p), _) => LayoutMode::Full { primary: p },
                _ => continue,
            };

            match current_layout {
                None => {
                    current_layout = Some(desired_layout);
                    layout_since = frame.time;
                }
                Some(active) if active == desired_layout => {
                    pending_layout = None;
                }
                Some(_) => {
                    if let Some((pending, started)) = pending_layout {
                        if pending == desired_layout && frame.time - started >= self.layout_hold {
                            spans.push(LayoutSpan {
                                start: layout_since,
                                end: frame.time,
                                layout: current_layout.unwrap(),
                            });
                            current_layout = Some(desired_layout);
                            layout_since = frame.time;
                            pending_layout = None;
                        }
                    } else {
                        pending_layout = Some((desired_layout, frame.time));
                    }
                }
            }
        }

        let final_layout = current_layout.ok_or_else(|| {
            MediaError::detection_failed(
                "Smart Split (Activity) could not determine a valid layout from detected faces",
            )
        })?;

        spans.push(LayoutSpan {
            start: layout_since,
            end: duration,
            layout: final_layout,
        });

        Ok(spans)
    }

    fn config_to_activity_cfg(&self) -> crate::intelligent::face_activity::FaceActivityConfig {
        crate::intelligent::face_activity::FaceActivityConfig {
            activity_window: self.config.face_activity_window,
            min_switch_duration: self.config.min_switch_duration,
            switch_margin: self.config.switch_margin,
            weight_mouth: 0.0, // mouth is not computed in visual-only path
            weight_motion: self.config.activity_weight_motion,
            weight_size: self.config.activity_weight_size_change,
            smoothing_alpha: self.config.activity_smoothing_window,
            enable_mouth_detection: false,
        }
    }
}

fn best_track(scores: &HashMap<u32, f64>) -> (u32, f64) {
    scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, v)| (*k, *v))
        .unwrap_or((0, 0.0))
}

fn second_best_track(scores: &HashMap<u32, f64>, primary: Option<u32>) -> (Option<u32>, f64) {
    let mut filtered: Vec<(u32, f64)> = scores
        .iter()
        .filter(|(id, _)| Some(**id) != primary)
        .map(|(k, v)| (*k, *v))
        .collect();
    filtered.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    filtered
        .get(0)
        .map(|(id, score)| (Some(*id), *score))
        .unwrap_or((None, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(time: f64, entries: &[(u32, f64)]) -> TimelineFrame {
        TimelineFrame {
            time,
            detections: Vec::new(),
            raw_activity: entries.iter().map(|(id, s)| (*id, *s)).collect(),
        }
    }

    fn planner() -> LayoutPlanner {
        LayoutPlanner::new(IntelligentCropConfig::default())
    }

    #[test]
    fn single_speaker_stays_full() {
        let frames = vec![
            frame(0.0, &[(1, 0.7)]),
            frame(0.2, &[(1, 0.8)]),
            frame(0.4, &[(1, 0.9)]),
        ];
        let plan = planner().plan(&frames, 0.6).unwrap();
        assert_eq!(plan.len(), 1);
        match plan[0].layout {
            LayoutMode::Full { primary } => assert_eq!(primary, 1),
            _ => panic!("expected full layout"),
        }
    }

    #[test]
    fn two_speakers_trigger_split() {
        let frames = vec![
            frame(0.0, &[(1, 0.8), (2, 0.3)]),
            frame(0.3, &[(1, 0.7), (2, 0.6)]),
            frame(0.6, &[(1, 0.6), (2, 0.65)]),
            frame(0.9, &[(1, 0.6), (2, 0.7)]),
        ];
        let plan = planner().plan(&frames, 1.2).unwrap();
        assert!(
            plan.iter().any(|s| matches!(s.layout, LayoutMode::Split { .. })),
            "expected a split span"
        );
    }

    #[test]
    fn primary_changes_only_with_margin() {
        let frames = vec![
            frame(0.0, &[(1, 0.7), (2, 0.6)]),
            frame(0.7, &[(1, 0.6), (2, 0.9)]),
            frame(1.4, &[(1, 0.5), (2, 0.92)]),
        ];
        let plan = planner().plan(&frames, 1.6).unwrap();
        let top_ids: Vec<u32> = plan
            .iter()
            .map(|span| match span.layout {
                LayoutMode::Full { primary } => primary,
                LayoutMode::Split { primary, .. } => primary,
            })
            .collect();
        assert!(top_ids.contains(&2));
    }

    #[test]
    fn three_speakers_keeps_primary_and_secondary_distinct() {
        let frames = vec![
            frame(0.0, &[(1, 0.65), (2, 0.62), (3, 0.4)]),
            frame(0.7, &[(1, 0.7), (2, 0.8), (3, 0.5)]),
            frame(1.4, &[(1, 0.72), (2, 0.82), (3, 0.55)]),
        ];
        let plan = planner().plan(&frames, 1.6).unwrap();
        let split_span = plan
            .iter()
            .find(|s| matches!(s.layout, LayoutMode::Split { .. }))
            .cloned()
            .expect("expected a split span");

        if let LayoutMode::Split { primary, secondary } = split_span.layout {
            assert_ne!(primary, secondary);
        }
    }
}

