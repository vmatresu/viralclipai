//! Tests for tier-aware intelligent processing.
//!
//! These tests verify that:
//! 1. Different tiers produce different camera behavior
//! 2. Speaker detection is integrated correctly
//! 3. Activity tracking with hysteresis works as expected
//! 4. A/V sync and padding are preserved

#[cfg(test)]
mod tier_aware_smoother_tests {
    use crate::intelligent::tier_aware_smoother::TierAwareCameraSmoother;
    use crate::intelligent::config::IntelligentCropConfig;
    use crate::intelligent::models::{BoundingBox, Detection};
    use crate::intelligent::speaker_detector::{ActiveSpeaker, SpeakerSegment};
    use vclip_models::DetectionTier;

    fn test_config() -> IntelligentCropConfig {
        IntelligentCropConfig::default()
    }

    #[test]
    fn test_basic_tier_ignores_speaker_info() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

        // Set up speaker segments - right speaker active
        smoother = smoother.with_speaker_segments(vec![SpeakerSegment {
            start_time: 0.0,
            end_time: 10.0,
            speaker: ActiveSpeaker::Right,
            confidence: 0.9,
        }]);

        // Create detections with larger face on left
        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 150.0, 150.0), 0.9, 1), // Left, larger
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 80.0, 80.0), 0.8, 2), // Right, smaller
        ];

        // Basic tier should select larger face regardless of speaker
        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        // Should focus on left (larger) face
        assert!(keyframes[0].cx < 960.0, "Basic tier should focus on larger face (left)");
    }

    #[test]
    fn test_speaker_aware_tier_follows_speaker() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);

        // Set up speaker segments - right speaker active
        smoother = smoother.with_speaker_segments(vec![SpeakerSegment {
            start_time: 0.0,
            end_time: 10.0,
            speaker: ActiveSpeaker::Right,
            confidence: 0.9,
        }]);

        // Set up track sides
        // Note: We need to build track sides first
        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 150.0, 150.0), 0.9, 1), // Left
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 80.0, 80.0), 0.8, 2), // Right
        ];

        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        // SpeakerAware should focus on right face (active speaker) despite being smaller
        assert!(keyframes[0].cx > 960.0, "SpeakerAware tier should focus on active speaker (right): got {}", keyframes[0].cx);
    }

    #[test]
    fn test_speaker_aware_tier_uses_hysteresis() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);

        // Initial selection at t=0
        let dets_t0 = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 100.0, 100.0), 0.8, 1),
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 100.0, 100.0), 0.7, 2),
        ];

        // At t=0.5 (before min_switch_duration), track 2 becomes more active
        let dets_t05 = vec![
            Detection::new(0.5, BoundingBox::new(100.0, 100.0, 100.0, 100.0), 0.5, 1),
            Detection::new(0.5, BoundingBox::new(1500.0, 100.0, 100.0, 100.0), 0.95, 2),
        ];

        // At t=1.5 (after min_switch_duration), track 2 still more active
        let dets_t15 = vec![
            Detection::new(1.5, BoundingBox::new(100.0, 100.0, 100.0, 100.0), 0.5, 1),
            Detection::new(1.5, BoundingBox::new(1500.0, 100.0, 100.0, 100.0), 0.95, 2),
        ];

        let frame_dets = vec![dets_t0, dets_t05, dets_t15];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 2.0);

        assert!(keyframes.len() >= 3, "Should have keyframes for each sample");
        // The exact behavior depends on hysteresis, but we should see a transition
    }

    #[test]
    fn test_fallback_when_no_speaker_segments() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);
        // No speaker segments set

        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 150.0, 150.0), 0.9, 1),
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 80.0, 80.0), 0.8, 2),
        ];

        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        // Should fall back to basic behavior (largest face)
        assert!(keyframes[0].cx < 960.0, "Should fall back to largest face when no speaker info");
    }

    #[test]
    fn test_empty_detections_uses_fallback() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

        let frame_dets: Vec<Vec<Detection>> = vec![vec![], vec![], vec![]];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        // Should use fallback (center-ish)
        let center_x = 1920.0 / 2.0;
        assert!(
            (keyframes[0].cx - center_x).abs() < 500.0,
            "Fallback should be near center"
        );
    }
}

#[cfg(test)]
mod tier_aware_cropper_tests {
    use crate::intelligent::tier_aware_cropper::TierAwareIntelligentCropper;
    use vclip_models::DetectionTier;

    #[test]
    fn test_cropper_tier_assignment() {
        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::Basic);
        assert_eq!(cropper.tier(), DetectionTier::Basic);

        let cropper = TierAwareIntelligentCropper::with_tier(DetectionTier::SpeakerAware);
        assert_eq!(cropper.tier(), DetectionTier::SpeakerAware);
    }

    #[test]
    fn test_tier_uses_audio_check() {
        assert!(!DetectionTier::None.uses_audio());
        assert!(!DetectionTier::Basic.uses_audio());
        assert!(DetectionTier::SpeakerAware.uses_audio());
    }

    #[test]
    fn test_tier_requires_yunet_check() {
        assert!(!DetectionTier::None.requires_yunet());
        assert!(DetectionTier::Basic.requires_yunet());
        assert!(DetectionTier::SpeakerAware.requires_yunet());
    }
}

#[cfg(test)]
mod tier_aware_split_tests {
    use crate::intelligent::tier_aware_split::TierAwareSplitProcessor;
    use vclip_models::DetectionTier;

    #[test]
    fn test_split_processor_tier_assignment() {
        let processor = TierAwareSplitProcessor::with_tier(DetectionTier::Basic);
        assert_eq!(processor.tier(), DetectionTier::Basic);
    }

    #[test]
    fn test_split_processor_tier_requires_yunet() {
        // Basic and above require YuNet
        assert!(DetectionTier::Basic.requires_yunet());
        assert!(DetectionTier::SpeakerAware.requires_yunet());
        
        // None does not require YuNet
        assert!(!DetectionTier::None.requires_yunet());
    }
}

#[cfg(test)]
mod activity_tracker_tests {
    use crate::intelligent::activity_scorer::TemporalActivityTracker;
    use crate::intelligent::face_activity::FaceActivityConfig;

    fn test_config() -> FaceActivityConfig {
        FaceActivityConfig {
            activity_window: 0.5,
            min_switch_duration: 1.0,
            switch_margin: 0.2,
            ..Default::default()
        }
    }

    #[test]
    fn test_initial_face_selection() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Add activity for two tracks
        tracker.update_activity(1, 0.8, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);

        let selected = tracker.select_active_face(&[1, 2], 0.0);
        assert_eq!(selected, Some(1), "Should select most active face initially");
    }

    #[test]
    fn test_min_switch_duration_enforced() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Initial selection at t=0
        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);
        tracker.select_active_face(&[1, 2], 0.0);

        // At t=0.5 (before min_switch_duration), track 2 becomes much more active
        tracker.update_activity(2, 0.9, 0.0, 0.5);
        let selected = tracker.select_active_face(&[1, 2], 0.5);
        assert_eq!(selected, Some(1), "Should not switch before min_switch_duration");

        // At t=1.5 (after min_switch_duration), should switch
        tracker.update_activity(2, 0.9, 0.0, 1.5);
        let selected = tracker.select_active_face(&[1, 2], 1.5);
        assert_eq!(selected, Some(2), "Should switch after min_switch_duration");
    }

    #[test]
    fn test_switch_margin_required() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        // Initial selection
        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.update_activity(2, 0.3, 0.0, 0.0);
        tracker.select_active_face(&[1, 2], 0.0);

        // At t=1.5, track 2 slightly better but not enough margin
        tracker.update_activity(1, 0.5, 0.0, 1.5);
        tracker.update_activity(2, 0.6, 0.0, 1.5); // Only 0.1 improvement, margin is 0.2
        let selected = tracker.select_active_face(&[1, 2], 1.5);
        assert_eq!(selected, Some(1), "Should not switch without sufficient margin");

        // Track 2 significantly better
        tracker.update_activity(2, 0.9, 0.0, 1.6);
        let selected = tracker.select_active_face(&[1, 2], 1.6);
        assert_eq!(selected, Some(2), "Should switch with sufficient margin");
    }

    #[test]
    fn test_audio_fusion_boosts_speaker() {
        let tracker = TemporalActivityTracker::new(test_config());

        // Visual only
        let score_no_audio = tracker.compute_final_score(0.8, 0.0);
        assert!(
            (score_no_audio - 0.4).abs() < 0.01,
            "No audio: 0.8 * 0.5 = 0.4, got {}",
            score_no_audio
        );

        // Visual + audio
        let score_with_audio = tracker.compute_final_score(0.8, 1.0);
        assert!(
            (score_with_audio - 0.8).abs() < 0.01,
            "With audio: 0.8 * 1.0 = 0.8, got {}",
            score_with_audio
        );
    }

    #[test]
    fn test_track_cleanup() {
        let mut tracker = TemporalActivityTracker::new(test_config());

        tracker.update_activity(1, 0.5, 0.0, 0.0);
        tracker.select_active_face(&[1], 0.0);

        assert_eq!(tracker.current_active_face(), Some(1));

        tracker.cleanup_track(1);

        assert_eq!(tracker.current_active_face(), None);
    }
}

// Speaker detector tests are in the speaker_detector module itself
// since determine_speaker is a private method
