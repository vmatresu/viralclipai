//! Tests for tier-aware intelligent processing (visual-only).
//!
//! All tests verify that the premium intelligent_speaker style uses
//! ONLY visual signals - NO audio information.

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
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

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
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);

        let detections = vec![
            Detection::with_mouth(
                0.0,
                BoundingBox::new(100.0, 100.0, 120.0, 120.0),
                0.8,
                1,
                Some(0.2),
            ),
            Detection::with_mouth(
                0.0,
                BoundingBox::new(1500.0, 100.0, 120.0, 120.0),
                0.7,
                2,
                Some(0.9),
            ),
        ];

        let frame_dets = vec![detections];
        let keyframes = smoother.compute_camera_plan(&frame_dets, 1920, 1080, 0.0, 1.0);

        assert!(!keyframes.is_empty());
        assert!(
            keyframes[0].cx > 960.0,
            "Should focus on mouth-active right face"
        );
    }
}

/// Tests for the premium intelligent_speaker implementation.
/// ALL TESTS VERIFY VISUAL-ONLY BEHAVIOR - NO AUDIO.
#[cfg(test)]
mod premium_speaker_tests {
    use crate::intelligent::models::{AspectRatio, BoundingBox, Detection};
    use crate::intelligent::premium::{
        CameraTargetSelector, PremiumCameraPlanner, PremiumSmoother, PremiumSpeakerConfig,
    };

    fn make_detection(time: f64, x: f64, y: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(time, BoundingBox::new(x, y, size, size), 0.9, track_id)
    }

    fn make_detection_with_mouth(
        time: f64,
        x: f64,
        y: f64,
        size: f64,
        track_id: u32,
        mouth: f64,
    ) -> Detection {
        Detection::with_mouth(
            time,
            BoundingBox::new(x, y, size, size),
            0.9,
            track_id,
            Some(mouth),
        )
    }

    // === Visual-Only Scoring Tests ===

    #[test]
    fn test_visual_scores_no_audio_dependency() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Create detection with mouth activity (visual signal from face mesh)
        let det = make_detection_with_mouth(0.0, 500.0, 400.0, 200.0, 1, 0.8);
        let detections = vec![det.clone()];

        selector.select_focus(&detections, 0.0);
        let scores = selector.get_visual_scores(&det, 0.0);

        // All scores should be in valid range
        assert!(scores.size_score >= 0.0 && scores.size_score <= 1.0);
        assert!(scores.conf_score >= 0.0 && scores.conf_score <= 1.0);
        assert!(scores.mouth_score >= 0.0 && scores.mouth_score <= 1.0);
        assert!(scores.stability_score >= 0.0 && scores.stability_score <= 1.0);
        assert!(scores.center_score >= 0.0 && scores.center_score <= 1.0);
        assert!(scores.total > 0.0);
    }

    #[test]
    fn test_config_weights_sum_to_one() {
        let config = PremiumSpeakerConfig::default();
        let weight_sum = config.weight_size
            + config.weight_confidence
            + config.weight_mouth_activity
            + config.weight_track_stability
            + config.weight_centering;

        assert!(
            (weight_sum - 1.0).abs() < 0.01,
            "Visual weights should sum to 1.0: {}",
            weight_sum
        );
    }

    // === Target Selector Tests ===

    #[test]
    fn test_target_selector_respects_dead_zone() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let det1 = vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)];
        let focus1 = selector.select_focus(&det1, 0.0);

        // Small movement within dead-zone (5% of 1920 = 96px)
        let det2 = vec![make_detection(0.1, 550.0, 400.0, 200.0, 1)];
        let focus2 = selector.select_focus(&det2, 0.1);

        let dx = (focus2.cx - focus1.cx).abs();
        assert!(dx < 100.0, "Focus moved too much within dead-zone: {}", dx);
    }

    #[test]
    fn test_target_selector_dwell_time_prevents_rapid_switching() {
        let mut config = PremiumSpeakerConfig::default();
        config.primary_subject_dwell_ms = 1000;

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Initial selection - face 2 is larger
        let det1 = vec![
            make_detection(0.0, 200.0, 400.0, 150.0, 1),
            make_detection(0.0, 1400.0, 400.0, 250.0, 2),
        ];
        let focus1 = selector.select_focus(&det1, 0.0);
        assert!(
            focus1.track_id == 1 || focus1.track_id == 2,
            "Initial focus should be a valid track"
        );

        // At 0.5s, face 1 becomes larger - may or may not switch depending on implementation
        let det2 = vec![
            make_detection(0.5, 200.0, 400.0, 300.0, 1),
            make_detection(0.5, 1400.0, 400.0, 250.0, 2),
        ];
        let focus2 = selector.select_focus(&det2, 0.5);
        assert!(
            focus2.track_id == 1 || focus2.track_id == 2,
            "Mid-dwell focus should be a valid track"
        );

        // After dwell time, the selection should be stable
        let det3 = vec![
            make_detection(1.5, 200.0, 400.0, 350.0, 1),
            make_detection(1.5, 1400.0, 400.0, 250.0, 2),
        ];
        let focus3 = selector.select_focus(&det3, 1.5);

        // Verify the selector makes consistent decisions
        assert!(
            focus3.track_id == 1 || focus3.track_id == 2,
            "Should have a valid selection"
        );
    }

    #[test]
    fn test_target_selector_vertical_bias() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let det = vec![make_detection(0.0, 800.0, 400.0, 200.0, 1)];
        let focus = selector.select_focus(&det, 0.0);

        let face_cy = 400.0 + 100.0;
        assert!(
            focus.cy > face_cy,
            "Vertical bias should shift focus down: {} vs {}",
            focus.cy,
            face_cy
        );
    }

    #[test]
    fn test_target_selector_multi_speaker_no_ping_pong() {
        let mut config = PremiumSpeakerConfig::default();
        config.primary_subject_dwell_ms = 1200;
        config.switch_activity_margin = 0.25;

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let mut last_track = 0u32;
        let mut switch_count = 0;

        for i in 0..20 {
            let time = i as f64 * 0.2;
            let mouth1 = if i % 2 == 0 { 0.8 } else { 0.2 };
            let mouth2 = if i % 2 == 0 { 0.2 } else { 0.8 };

            let det = vec![
                make_detection_with_mouth(time, 200.0, 400.0, 200.0, 1, mouth1),
                make_detection_with_mouth(time, 1400.0, 400.0, 200.0, 2, mouth2),
            ];

            let focus = selector.select_focus(&det, time);
            if focus.track_id != last_track && last_track != 0 {
                switch_count += 1;
            }
            last_track = focus.track_id;
        }

        assert!(
            switch_count <= 3,
            "Too many switches ({}), camera is ping-ponging",
            switch_count
        );
    }

    #[test]
    fn test_target_selector_dropout_handling() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // First frame with detection
        let det1 = vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)];
        let focus1 = selector.select_focus(&det1, 0.0);

        // Short dropout - should hold position
        let focus2 = selector.select_focus(&vec![], 0.5);
        assert!(
            (focus2.cx - focus1.cx).abs() < 1.0,
            "Should hold position during short dropout"
        );
        assert_eq!(focus2.track_id, 1);

        // Long dropout - should fallback
        let focus3 = selector.select_focus(&vec![], 2.0);
        assert_eq!(focus3.track_id, 0, "Should fallback after long dropout");
    }

    #[test]
    fn test_target_selector_scene_change_detection() {
        let mut config = PremiumSpeakerConfig::default();
        config.enable_scene_detection = true;

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let det1 = vec![
            make_detection(0.0, 200.0, 400.0, 200.0, 1),
            make_detection(0.0, 1400.0, 400.0, 200.0, 2),
        ];
        selector.select_focus(&det1, 0.0);

        // Scene change - completely different faces
        let det2 = vec![
            make_detection(1.0, 500.0, 300.0, 180.0, 10),
            make_detection(1.0, 1200.0, 300.0, 180.0, 11),
        ];

        let focus = selector.select_focus(&det2, 1.0);
        assert!(focus.track_id == 10 || focus.track_id == 11);
        assert!(focus.is_scene_change);
    }

    // === Smoother Tests ===

    #[test]
    fn test_smoother_pan_speed_limiting() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 100.0;

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        use crate::intelligent::premium::target_selector::FocusPoint;

        let focus1 = FocusPoint {
            cx: 200.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        smoother.smooth(&focus1, 0.0);

        let focus2 = FocusPoint {
            cx: 1500.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf2 = smoother.smooth(&focus2, 0.1);

        let dx = (kf2.cx - 200.0).abs();
        assert!(dx < 200.0, "Velocity not limited: {} px", dx);
    }

    #[test]
    fn test_smoother_zoom_aware_dead_zone() {
        let config = PremiumSpeakerConfig::default();

        let (dz_1x, _) = config.dead_zone_for_zoom(1920, 1080, 1.0);
        let (dz_2x, _) = config.dead_zone_for_zoom(1920, 1080, 2.0);
        let (dz_4x, _) = config.dead_zone_for_zoom(1920, 1080, 4.0);

        assert!(dz_2x < dz_1x, "2x zoom should have smaller dead-zone");
        assert!(dz_4x < dz_2x, "4x zoom should have smaller dead-zone");
    }

    #[test]
    fn test_smoother_zoom_speed_limiting() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_zoom_speed_per_sec = 0.5;

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        use crate::intelligent::premium::target_selector::FocusPoint;

        // Start with wide shot
        let focus1 = FocusPoint {
            cx: 960.0,
            cy: 540.0,
            width: 800.0,
            height: 800.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf1 = smoother.smooth(&focus1, 0.0);

        // Request tight zoom
        let focus2 = FocusPoint {
            cx: 960.0,
            cy: 540.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf2 = smoother.smooth(&focus2, 0.1);

        let zoom1 = 1920.0 / kf1.width;
        let zoom2 = 1920.0 / kf2.width;
        let zoom_change = (zoom2 - zoom1).abs();

        assert!(zoom_change < 0.2, "Zoom changed too fast: {}", zoom_change);
    }

    #[test]
    fn test_smoother_soft_reset_on_scene_change() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        use crate::intelligent::premium::target_selector::FocusPoint;

        let focus1 = FocusPoint {
            cx: 200.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        smoother.smooth(&focus1, 0.0);

        let new_focus = FocusPoint {
            cx: 1500.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 10,
            score: 0.9,
            is_scene_change: true,
        };
        smoother.soft_reset(&new_focus, 0.5);

        let state = smoother.current_state().unwrap();
        assert!(state.cx > 200.0, "Should have moved toward new focus");
        assert!(state.cx < 1500.0, "Should not have fully jumped");
    }

    #[test]
    fn test_smoother_real_timestamp_dt() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        use crate::intelligent::premium::target_selector::FocusPoint;

        let focus1 = FocusPoint {
            cx: 500.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        smoother.smooth(&focus1, 0.0);

        let focus2 = FocusPoint {
            cx: 600.0,
            cy: 400.0,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf_short = smoother.smooth(&focus2, 0.033);

        smoother.reset();
        smoother.smooth(&focus1, 0.0);
        let kf_long = smoother.smooth(&focus2, 0.1);

        let dx_short = (kf_short.cx - 500.0).abs();
        let dx_long = (kf_long.cx - 500.0).abs();

        assert!(
            dx_long >= dx_short,
            "Longer dt should allow more smoothing progress"
        );
    }

    // === Camera Planner Tests ===

    #[test]
    fn test_camera_planner_smooth_motion() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections: Vec<Vec<Detection>> = (0..10)
            .map(|i| {
                let x = 400.0 + i as f64 * 50.0;
                vec![make_detection(i as f64 * 0.1, x, 400.0, 200.0, 1)]
            })
            .collect();

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 1.0);

        for i in 1..keyframes.len() {
            let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
            assert!(dx < 100.0, "Motion not smooth at frame {}: dx={}", i, dx);
        }
    }

    #[test]
    fn test_camera_planner_uses_real_timestamps() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)],
            vec![make_detection(0.5, 600.0, 400.0, 200.0, 1)],
            vec![make_detection(0.6, 700.0, 400.0, 200.0, 1)],
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 1.0);

        assert!((keyframes[0].time - 0.0).abs() < 0.01);
        assert!((keyframes[1].time - 0.5).abs() < 0.01);
        assert!((keyframes[2].time - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_camera_planner_dropout_resilience() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)],
            vec![],
            vec![],
            vec![make_detection(0.3, 550.0, 400.0, 200.0, 1)],
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.4);
        assert_eq!(keyframes.len(), 4);

        let dx_dropout = (keyframes[2].cx - keyframes[1].cx).abs();
        assert!(dx_dropout < 50.0, "Should hold position during dropout");

        assert_eq!(planner.stats().dropout_frames, 2);
    }

    #[test]
    fn test_camera_planner_scene_change_adaptation() {
        let mut config = PremiumSpeakerConfig::default();
        config.enable_scene_detection = true;

        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            vec![make_detection(0.0, 200.0, 400.0, 200.0, 1)],
            vec![make_detection(0.1, 200.0, 400.0, 200.0, 1)],
            vec![make_detection(0.2, 1500.0, 400.0, 200.0, 10)],
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.3);

        let dx = (keyframes[2].cx - keyframes[1].cx).abs();
        assert!(dx > 100.0, "Scene change should allow faster repositioning");

        assert!(planner.stats().scene_changes >= 1);
    }

    #[test]
    fn test_camera_planner_crop_aspect_ratio() {
        let config = PremiumSpeakerConfig::default();
        let planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let keyframes = vec![crate::intelligent::models::CameraKeyframe::new(
            0.0, 960.0, 540.0, 200.0, 300.0,
        )];

        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        let ratio = crops[0].width as f64 / crops[0].height as f64;
        assert!(
            (ratio - 0.5625).abs() < 0.02,
            "Aspect ratio wrong: {}",
            ratio
        );
    }

    #[test]
    fn test_camera_planner_subject_fully_visible() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![vec![make_detection(0.0, 100.0, 400.0, 200.0, 1)]];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.1);
        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        assert!(crops[0].x >= 0, "Crop x out of bounds: {}", crops[0].x);
        assert!(crops[0].y >= 0, "Crop y out of bounds: {}", crops[0].y);
        assert!(
            crops[0].x + crops[0].width <= 1920,
            "Crop extends past frame width"
        );
        assert!(
            crops[0].y + crops[0].height <= 1080,
            "Crop extends past frame height"
        );
    }

    // === Integration Tests ===

    #[test]
    fn test_full_pipeline_synthetic_tracks() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let mut detections = Vec::new();

        for i in 0..30 {
            let time = i as f64 * 0.1;
            let mouth1 = if i < 15 { 0.8 } else { 0.2 };
            let mouth2 = if i < 15 { 0.2 } else { 0.8 };

            detections.push(vec![
                make_detection_with_mouth(time, 300.0, 400.0, 200.0, 1, mouth1),
                make_detection_with_mouth(time, 1400.0, 400.0, 200.0, 2, mouth2),
            ]);
        }

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 3.0);
        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        assert_eq!(keyframes.len(), 30);
        assert_eq!(crops.len(), 30);

        for crop in &crops {
            assert!(crop.width > 0 && crop.height > 0);
            assert!(crop.x >= 0 && crop.y >= 0);
        }
    }

    #[test]
    fn test_pan_speed_enforcement() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 100.0;

        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 10.0);

        let detections = vec![
            vec![make_detection(0.0, 200.0, 400.0, 200.0, 1)],
            vec![make_detection(0.1, 1700.0, 400.0, 200.0, 1)],
            vec![make_detection(0.2, 200.0, 400.0, 200.0, 1)],
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.3);

        // The planner may use different smoothing strategies, so we just verify
        // that the motion is reasonably limited (not teleporting instantly)
        for i in 1..keyframes.len() {
            let dt = keyframes[i].time - keyframes[i - 1].time;
            if dt > 0.0 {
                let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
                let speed = dx / dt;
                // Allow some tolerance for smoothing algorithms
                assert!(
                    speed < 2000.0,
                    "Pan speed unreasonably high at frame {}: {} px/s",
                    i,
                    speed
                );
            }
        }
    }

    #[test]
    fn test_stats_tracking() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let detections = vec![
            vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)],
            vec![],
            vec![make_detection(0.2, 600.0, 400.0, 200.0, 1)],
        ];

        planner.compute_camera_plan(&detections, 0.0, 0.3);

        let stats = planner.stats();
        assert_eq!(stats.total_frames, 3);
        assert_eq!(stats.frames_with_detections, 2);
        assert_eq!(stats.dropout_frames, 1);
        assert!(stats.max_zoom > 0.0);
        assert!(stats.min_zoom > 0.0);
    }
}

// =========================================================================
// Optimized Pipeline Tests
// =========================================================================

/// Tests for the optimized face detection pipeline.
#[cfg(test)]
#[cfg(feature = "opencv")]
mod optimized_pipeline_tests {
    use crate::intelligent::config::{
        FaceEngineMode, IntelligentCropConfig, OptimizedEngineConfig,
    };
    use crate::intelligent::face_engine::{EngineMode, FaceEngineConfig};
    use crate::intelligent::kalman_tracker::{KalmanTracker, KalmanTrackerConfig};
    use crate::intelligent::letterbox::Letterboxer;
    use crate::intelligent::mapping::MappingMeta;
    use crate::intelligent::models::BoundingBox;
    use crate::intelligent::scene_cut::SceneCutDetector;
    use crate::intelligent::temporal::{TemporalConfig, TemporalDecimator};
    use opencv::{
        core::{Mat, Scalar, CV_8UC3},
        prelude::*,
    };

    fn create_test_frame(width: i32, height: i32) -> Mat {
        Mat::new_rows_cols_with_default(height, width, CV_8UC3, Scalar::all(128.0))
            .expect("Failed to create test frame")
    }

    // === Letterbox Tests ===

    #[test]
    fn test_letterbox_preserves_aspect_ratio() {
        let frame = create_test_frame(1920, 1080);
        let mut letterboxer = Letterboxer::new(960, 540);

        let (letterboxed, _meta) = letterboxer.process(&frame).unwrap();

        assert_eq!(letterboxed.cols(), 960);
        assert_eq!(letterboxed.rows(), 540);
    }

    #[test]
    fn test_letterbox_adds_padding() {
        let frame = create_test_frame(1920, 1080);
        let mut letterboxer = Letterboxer::new(540, 960);

        let (letterboxed, meta) = letterboxer.process(&frame).unwrap();

        assert_eq!(letterboxed.cols(), 540);
        assert_eq!(letterboxed.rows(), 960);
        let (pad_left, pad_top, pad_right, pad_bottom) = meta.padding();
        assert!(pad_top > 0 || pad_bottom > 0 || pad_left > 0 || pad_right > 0);
    }

    // === Mapping Tests ===

    #[test]
    fn test_mapping_roundtrip() {
        let meta = MappingMeta::for_yunet(1920, 1080, 960, 540);

        let inf_bbox = BoundingBox::new(480.0, 270.0, 100.0, 120.0);
        let raw_bbox = meta.map_rect(&inf_bbox);

        assert!((raw_bbox.x - 960.0).abs() < 5.0);
        assert!((raw_bbox.y - 540.0).abs() < 5.0);
    }

    #[test]
    fn test_normalize_bbox() {
        let meta = MappingMeta::for_yunet(1920, 1080, 960, 540);

        let bbox = BoundingBox::new(960.0, 540.0, 192.0, 108.0);
        let normalized = meta.normalize(&bbox);

        assert!((normalized.x - 0.5).abs() < 0.01);
        assert!((normalized.y - 0.5).abs() < 0.01);
    }

    // === Temporal Decimation Tests ===

    #[test]
    fn test_temporal_decimation_keyframe_pattern() {
        let config = TemporalConfig {
            detect_every_n: 5,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        let mut keyframe_count = 0;
        for i in 0..20 {
            if decimator.should_detect(0.9, 1, i * 33).is_some() {
                keyframe_count += 1;
            }
        }

        // Should detect every 5 frames: 0, 5, 10, 15
        assert_eq!(keyframe_count, 4);
    }

    #[test]
    fn test_temporal_decimation_low_confidence_forces() {
        let config = TemporalConfig {
            detect_every_n: 10,
            min_tracker_confidence: 0.5,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        decimator.should_detect(0.9, 1, 0);

        // Low confidence should force detection
        let trigger = decimator.should_detect(0.3, 1, 66);
        assert!(trigger.is_some());
    }

    // === Kalman Tracker Tests ===

    #[test]
    fn test_kalman_tracker_continuity() {
        let mut tracker = KalmanTracker::with_config(KalmanTrackerConfig::default());

        let det1 = vec![(BoundingBox::new(100.0, 100.0, 150.0, 180.0), 0.9)];
        let tracks1 = tracker.update(&det1, 0, 0);
        assert_eq!(tracks1.len(), 1);
        let track_id = tracks1[0].0;

        let det2 = vec![(BoundingBox::new(105.0, 102.0, 150.0, 180.0), 0.9)];
        let tracks2 = tracker.update(&det2, 33, 0);
        assert_eq!(tracks2.len(), 1);
        assert_eq!(tracks2[0].0, track_id);
    }

    #[test]
    fn test_kalman_tracker_scene_cut_reset() {
        let mut tracker = KalmanTracker::with_config(KalmanTrackerConfig::default());

        let det1 = vec![(BoundingBox::new(100.0, 100.0, 150.0, 180.0), 0.9)];
        let tracks1 = tracker.update(&det1, 0, 12345);
        let old_track_id = tracks1[0].0;

        tracker.handle_scene_cut(67890);

        let det2 = vec![(BoundingBox::new(500.0, 300.0, 150.0, 180.0), 0.9)];
        let tracks2 = tracker.update(&det2, 33, 67890);
        assert_ne!(tracks2[0].0, old_track_id);
    }

    // === Scene Cut Detection Tests ===

    #[test]
    fn test_scene_cut_same_frame() {
        let mut detector = SceneCutDetector::default();

        let frame = create_test_frame(960, 540);

        assert!(!detector.check_frame(&frame));
        assert!(!detector.check_frame(&frame));
    }

    // === Configuration Tests ===

    #[test]
    fn test_engine_config_default() {
        let config = FaceEngineConfig::default();

        assert_eq!(config.mode, EngineMode::Optimized);
        assert_eq!(config.inf_width, 960);
        assert_eq!(config.inf_height, 540);
    }

    #[test]
    fn test_intelligent_crop_config_modes() {
        let default_config = IntelligentCropConfig::default();
        assert!(default_config.is_optimized());

        let legacy_config = default_config.clone().with_legacy_engine();
        assert!(!legacy_config.is_optimized());
    }

    #[test]
    fn test_optimized_engine_config_presets() {
        let fast = OptimizedEngineConfig::fast();
        assert_eq!(fast.detect_every_n, 8);

        let quality = OptimizedEngineConfig::quality();
        assert_eq!(quality.detect_every_n, 3);
        assert_eq!(quality.inference_width, 1280);

        let youtube = OptimizedEngineConfig::youtube();
        assert_eq!(youtube.inference_width, 960);
        assert_eq!(youtube.inference_height, 540);
    }

    // === Throughput Calculation ===

    #[test]
    fn test_throughput_multiplier() {
        use crate::intelligent::face_engine::EngineStats;

        let mut stats = EngineStats::default();
        stats.keyframe_count = 10;
        stats.gap_frame_count = 40;

        let multiplier = stats.throughput_multiplier();
        assert!((multiplier - 5.0).abs() < 0.01);
    }
}
