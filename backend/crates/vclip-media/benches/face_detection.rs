//! Face Detection Benchmarks
//!
//! Compares performance between optimized and legacy face detection pipelines.
//!
//! # Running Benchmarks
//! ```bash
//! cargo bench --package vclip-media --bench face_detection
//! ```
//!
//! # Metrics Measured
//! - Throughput (frames/second)
//! - Latency per frame
//! - Memory allocations
//! - Keyframe vs gap frame ratio

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

#[cfg(feature = "opencv")]
mod benchmarks {
    use super::*;
    use opencv::{
        core::{Mat, Scalar, CV_8UC3},
        prelude::*,
    };
    use vclip_media::intelligent::{
        FaceEngineConfig, FaceInferenceEngine, FrameConverter, IntelligentCropConfig,
        KalmanTracker, KalmanTrackerConfig, Letterboxer, MappingMeta, SceneCutDetector,
        TemporalConfig, TemporalDecimator,
    };

    /// Create a synthetic BGR frame for benchmarking.
    fn create_test_frame(width: i32, height: i32) -> Mat {
        let mut frame = Mat::new_rows_cols_with_default(height, width, CV_8UC3, Scalar::all(128.0))
            .expect("Failed to create test frame");

        // Add some variation to simulate real video
        for y in 0..height {
            for x in 0..width {
                let pixel = frame.at_2d_mut::<opencv::core::Vec3b>(y, x).unwrap();
                pixel[0] = ((x * 7 + y * 11) % 256) as u8;
                pixel[1] = ((x * 13 + y * 17) % 256) as u8;
                pixel[2] = ((x * 19 + y * 23) % 256) as u8;
            }
        }

        frame
    }

    /// Benchmark letterbox preprocessing.
    pub fn bench_letterbox(c: &mut Criterion) {
        let mut group = c.benchmark_group("letterbox");
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(5));

        let resolutions = [(1920, 1080), (1280, 720), (640, 360)];

        for (width, height) in resolutions {
            let frame = create_test_frame(width, height);
            let mut letterboxer = Letterboxer::new(960, 540);

            group.throughput(Throughput::Elements(1));
            group.bench_with_input(
                BenchmarkId::new("process", format!("{}x{}", width, height)),
                &frame,
                |b, frame| {
                    b.iter(|| {
                        let result = letterboxer.process(black_box(frame));
                        black_box(result)
                    })
                },
            );
        }

        group.finish();
    }

    /// Benchmark mapping meta computation.
    pub fn bench_mapping(c: &mut Criterion) {
        let mut group = c.benchmark_group("mapping");
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        let resolutions = [(1920, 1080), (1280, 720), (640, 360)];

        for (width, height) in resolutions {
            group.throughput(Throughput::Elements(1));
            group.bench_with_input(
                BenchmarkId::new("compute", format!("{}x{}", width, height)),
                &(width, height),
                |b, &(w, h)| {
                    b.iter(|| {
                        let meta = MappingMeta::for_yunet(w, h, 960, 540);
                        black_box(meta)
                    })
                },
            );
        }

        group.finish();
    }

    /// Benchmark Kalman tracker update.
    pub fn bench_kalman_tracker(c: &mut Criterion) {
        let mut group = c.benchmark_group("kalman_tracker");
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(5));

        let face_counts = [1, 2, 5, 10];

        for num_faces in face_counts {
            let mut tracker = KalmanTracker::with_config(KalmanTrackerConfig::default());

            // Create test detections
            let detections: Vec<(vclip_media::intelligent::BoundingBox, f64)> = (0..num_faces)
                .map(|i| {
                    let x = 100.0 + (i as f64) * 200.0;
                    let bbox = vclip_media::intelligent::BoundingBox::new(x, 200.0, 150.0, 180.0);
                    (bbox, 0.9)
                })
                .collect();

            group.throughput(Throughput::Elements(1));
            group.bench_with_input(
                BenchmarkId::new("update", format!("{}_faces", num_faces)),
                &detections,
                |b, dets| {
                    b.iter(|| {
                        let result = tracker.update(black_box(dets), 0, 12345);
                        black_box(result)
                    })
                },
            );
        }

        group.finish();
    }

    /// Benchmark temporal decimator decision making.
    pub fn bench_temporal_decimator(c: &mut Criterion) {
        let mut group = c.benchmark_group("temporal_decimator");
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        let configs = [
            ("fast", TemporalConfig::fast()),
            ("default", TemporalConfig::default()),
            ("quality", TemporalConfig::quality()),
        ];

        for (name, config) in configs {
            let mut decimator = TemporalDecimator::new(config);

            group.throughput(Throughput::Elements(1));
            group.bench_with_input(BenchmarkId::new("should_detect", name), &(), |b, _| {
                b.iter(|| {
                    let result = decimator.should_detect(black_box(0.8), black_box(2), black_box(0));
                    black_box(result)
                })
            });
        }

        group.finish();
    }

    /// Benchmark scene cut detection.
    pub fn bench_scene_cut(c: &mut Criterion) {
        let mut group = c.benchmark_group("scene_cut");
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(5));

        let frame = create_test_frame(960, 540);
        let mut detector = SceneCutDetector::default();

        group.throughput(Throughput::Elements(1));
        group.bench_function("check_frame", |b| {
            b.iter(|| {
                let result = detector.check_frame(black_box(&frame));
                black_box(result)
            })
        });

        group.bench_function("compute_scene_hash", |b| {
            b.iter(|| {
                let result = detector.compute_scene_hash(black_box(&frame));
                black_box(result)
            })
        });

        group.finish();
    }

    /// Benchmark frame converter with buffer pooling.
    pub fn bench_frame_converter(c: &mut Criterion) {
        let mut group = c.benchmark_group("frame_converter");
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(5));

        let frame = create_test_frame(1920, 1080);
        let mut converter = FrameConverter::for_yunet();

        // Warm up to establish buffer pool
        for _ in 0..10 {
            let _ = converter.convert_bgr(&frame);
        }

        group.throughput(Throughput::Elements(1));
        group.bench_function("convert_bgr_1080p", |b| {
            b.iter(|| {
                let result = converter.convert_bgr(black_box(&frame));
                black_box(result)
            })
        });

        group.finish();
    }

    /// Benchmark complete inference pipeline (without actual YuNet).
    pub fn bench_pipeline_overhead(c: &mut Criterion) {
        let mut group = c.benchmark_group("pipeline_overhead");
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(5));

        let frame = create_test_frame(1920, 1080);

        // Benchmark preprocessing pipeline without YuNet inference
        let mut letterboxer = Letterboxer::new(960, 540);
        let mut tracker = KalmanTracker::with_config(KalmanTrackerConfig::default());
        let mut decimator = TemporalDecimator::new(TemporalConfig::default());
        let mut scene_cut = SceneCutDetector::default();

        group.throughput(Throughput::Elements(1));
        group.bench_function("full_preprocessing", |b| {
            let mut frame_idx = 0u64;
            b.iter(|| {
                // Scene cut check
                let is_scene_cut = scene_cut.check_frame(&frame);
                if is_scene_cut {
                    decimator.notify_scene_cut(0);
                    tracker.handle_scene_cut(0);
                }

                // Decimation decision
                let trigger = decimator.should_detect(0.8, 2, frame_idx);

                if trigger.is_some() {
                    // Letterbox (simulating keyframe)
                    let (letterboxed, meta) = letterboxer.process(&frame).unwrap();
                    black_box((letterboxed, meta));
                } else {
                    // Kalman predict (simulating gap frame)
                    let predicted = tracker.predict(frame_idx);
                    black_box(predicted);
                }

                frame_idx += 33; // ~30fps timestamps
            })
        });

        group.finish();
    }
}

#[cfg(feature = "opencv")]
criterion_group!(
    benches,
    benchmarks::bench_letterbox,
    benchmarks::bench_mapping,
    benchmarks::bench_kalman_tracker,
    benchmarks::bench_temporal_decimator,
    benchmarks::bench_scene_cut,
    benchmarks::bench_frame_converter,
    benchmarks::bench_pipeline_overhead,
);

#[cfg(feature = "opencv")]
criterion_main!(benches);

#[cfg(not(feature = "opencv"))]
fn main() {
    eprintln!("Benchmarks require the 'opencv' feature");
}
