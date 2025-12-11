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

/// Tests for the premium intelligent_speaker implementation.
#[cfg(test)]
mod premium_speaker_tests {
    use crate::intelligent::models::{AspectRatio, BoundingBox, Detection};
    use crate::intelligent::premium::{
        CameraTargetSelector, PremiumCameraPlanner, PremiumSpeakerConfig,
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

    // === Target Selector Tests ===

    #[test]
    fn test_target_selector_respects_dead_zone() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Initial position
        let det1 = vec![make_detection(0.0, 500.0, 400.0, 200.0, 1)];
        let focus1 = selector.select_focus(&det1, 0.0);

        // Small movement within dead-zone (5% of 1920 = 96px)
        let det2 = vec![make_detection(0.1, 550.0, 400.0, 200.0, 1)]; // +50px
        let focus2 = selector.select_focus(&det2, 0.1);

        // Focus should be relatively stable
        let dx = (focus2.cx - focus1.cx).abs();
        assert!(dx < 100.0, "Focus moved too much within dead-zone: {}", dx);
    }

    #[test]
    fn test_target_selector_dwell_time_prevents_rapid_switching() {
        let mut config = PremiumSpeakerConfig::default();
        config.primary_subject_dwell_ms = 1000; // 1 second dwell

        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        // Initial selection - face 2 is larger
        let det1 = vec![
            make_detection(0.0, 200.0, 400.0, 150.0, 1),
            make_detection(0.0, 1400.0, 400.0, 250.0, 2),
        ];
        let focus1 = selector.select_focus(&det1, 0.0);
        assert_eq!(focus1.track_id, 2, "Should select larger face initially");

        // At 0.5s, face 1 becomes larger - should NOT switch (dwell time)
        let det2 = vec![
            make_detection(0.5, 200.0, 400.0, 300.0, 1), // Now larger
            make_detection(0.5, 1400.0, 400.0, 250.0, 2),
        ];
        let focus2 = selector.select_focus(&det2, 0.5);
        assert_eq!(focus2.track_id, 2, "Should NOT switch before dwell time");

        // At 1.5s (after dwell), face 1 still larger - should switch
        let det3 = vec![
            make_detection(1.5, 200.0, 400.0, 350.0, 1),
            make_detection(1.5, 1400.0, 400.0, 250.0, 2),
        ];
        let focus3 = selector.select_focus(&det3, 1.5);
        assert_eq!(focus3.track_id, 1, "Should switch after dwell time");
    }

    #[test]
    fn test_target_selector_vertical_bias() {
        let config = PremiumSpeakerConfig::default();
        let mut selector = CameraTargetSelector::new(config, 1920, 1080);

        let det = vec![make_detection(0.0, 800.0, 400.0, 200.0, 1)];
        let focus = selector.select_focus(&det, 0.0);

        // Face center is at y=500 (400 + 200/2)
        // With vertical bias, focus.cy should be shifted down
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

        // Simulate two speakers alternating briefly
        let mut last_track = 0u32;
        let mut switch_count = 0;

        for i in 0..20 {
            let time = i as f64 * 0.2; // 0.2s intervals
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

        // With 1.2s dwell time and 0.2s intervals, we should have very few switches
        assert!(
            switch_count <= 3,
            "Too many switches ({}), camera is ping-ponging",
            switch_count
        );
    }

    // === Camera Planner Tests ===

    #[test]
    fn test_camera_planner_smooth_motion() {
        let config = PremiumSpeakerConfig::default();
        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        // Subject moves gradually
        let detections: Vec<Vec<Detection>> = (0..10)
            .map(|i| {
                let x = 400.0 + i as f64 * 50.0;
                vec![make_detection(i as f64 * 0.1, x, 400.0, 200.0, 1)]
            })
            .collect();

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 1.0);

        // Check that motion is smooth (no large jumps)
        for i in 1..keyframes.len() {
            let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
            assert!(
                dx < 100.0,
                "Motion not smooth at frame {}: dx={}",
                i,
                dx
            );
        }
    }

    #[test]
    fn test_camera_planner_velocity_limiting() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 200.0; // Slow for testing

        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        // Large instant jump
        let detections = vec![
            vec![make_detection(0.0, 200.0, 400.0, 200.0, 1)],
            vec![make_detection(0.1, 1500.0, 400.0, 200.0, 1)], // 1300px jump
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.2);

        // Movement should be limited
        let dx = (keyframes[1].cx - keyframes[0].cx).abs();
        assert!(
            dx < 500.0,
            "Velocity not limited: {} px movement in 0.1s",
            dx
        );
    }

    #[test]
    fn test_camera_planner_crop_aspect_ratio() {
        let config = PremiumSpeakerConfig::default();
        let planner = PremiumCameraPlanner::new(config, 1920, 1080, 30.0);

        let keyframes = vec![crate::intelligent::models::CameraKeyframe::new(
            0.0, 960.0, 540.0, 200.0, 300.0,
        )];

        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        // Should be 9:16 aspect ratio
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

        // Subject near edge
        let detections = vec![vec![make_detection(0.0, 100.0, 400.0, 200.0, 1)]];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.1);
        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        // Crop should be within frame bounds
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

        // Simulate a 3-second clip with two speakers
        let mut detections = Vec::new();

        for i in 0..30 {
            let time = i as f64 * 0.1;
            let mouth1 = if i < 15 { 0.8 } else { 0.2 }; // Speaker 1 active first half
            let mouth2 = if i < 15 { 0.2 } else { 0.8 }; // Speaker 2 active second half

            detections.push(vec![
                make_detection_with_mouth(time, 300.0, 400.0, 200.0, 1, mouth1),
                make_detection_with_mouth(time, 1400.0, 400.0, 200.0, 2, mouth2),
            ]);
        }

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 3.0);
        let crops = planner.compute_crop_windows(&keyframes, &AspectRatio::PORTRAIT);

        // Should have keyframes for all frames
        assert_eq!(keyframes.len(), 30);
        assert_eq!(crops.len(), 30);

        // All crops should be valid
        for crop in &crops {
            assert!(crop.width > 0 && crop.height > 0);
            assert!(crop.x >= 0 && crop.y >= 0);
        }
    }

    #[test]
    fn test_pan_speed_enforcement() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 100.0;

        let mut planner = PremiumCameraPlanner::new(config, 1920, 1080, 10.0); // 10 fps

        // Create detections with large position changes
        let detections = vec![
            vec![make_detection(0.0, 200.0, 400.0, 200.0, 1)],
            vec![make_detection(0.1, 1700.0, 400.0, 200.0, 1)], // 1500px jump
            vec![make_detection(0.2, 200.0, 400.0, 200.0, 1)],  // Jump back
        ];

        let keyframes = planner.compute_camera_plan(&detections, 0.0, 0.3);

        // Check that pan speed is limited
        for i in 1..keyframes.len() {
            let dt = keyframes[i].time - keyframes[i - 1].time;
            if dt > 0.0 {
                let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
                let speed = dx / dt;
                // Allow some tolerance due to smoothing
                assert!(
                    speed < 500.0,
                    "Pan speed too high at frame {}: {} px/s",
                    i,
                    speed
                );
            }
        }
    }
}

