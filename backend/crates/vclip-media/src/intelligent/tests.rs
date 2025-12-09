//! Tests for tier-aware intelligent processing (visual-only).

#[cfg(test)]
mod tier_aware_smoother_tests {
    use crate::intelligent::config::IntelligentCropConfig;
    use crate::intelligent::models::{BoundingBox, Detection};
    use crate::intelligent::tier_aware_smoother::TierAwareCameraSmoother;
    use vclip_models::DetectionTier;

    fn test_config() -> IntelligentCropConfig {
        IntelligentCropConfig::default()
    }

    #[test]
    fn test_basic_tier_prefers_largest_face() {
        let config = test_config();
        let smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 150.0, 150.0), 0.9, 1),
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 80.0, 80.0), 0.8, 2),
        ];

        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        assert!(keyframes[0].cx < 960.0, "Should focus on larger left face");
    }

    #[test]
    fn test_speaker_aware_prefers_mouth_activity() {
        let config = test_config();
        let smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);

        let detections = vec![
            Detection::with_mouth(0.0, BoundingBox::new(100.0, 100.0, 120.0, 120.0), 0.8, 1, Some(0.2)),
            Detection::with_mouth(0.0, BoundingBox::new(1500.0, 100.0, 120.0, 120.0), 0.7, 2, Some(0.9)),
        ];

        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        assert!(keyframes[0].cx > 960.0, "Should focus on mouth-active right face");
    }
}

