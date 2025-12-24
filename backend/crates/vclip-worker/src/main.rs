//! Video processing worker binary.

use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use vclip_queue::JobQueue;
use vclip_worker::{JobExecutor, WorkerConfig};

#[tokio::main]
async fn main() {
    // Install rustls crypto provider (required for TLS/HTTPS)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing with colored output for dev, JSON for production
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v.to_lowercase() == "json")
        .unwrap_or(false);

    let env_filter = EnvFilter::from_default_env()
        .add_directive("vclip=info".parse().unwrap())
        .add_directive("ort=warn".parse().unwrap())
        .add_directive("onnxruntime=warn".parse().unwrap());

    if use_json {
        tracing_subscriber::registry()
            .with(fmt::layer().json())
            .with(env_filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_ansi(true)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false)
            )
            .with(env_filter)
            .init();
    }

    info!("Starting vclip-worker");

    // Load configuration
    let config = WorkerConfig::from_env();
    info!("Worker config: {:?}", config);

    // Configure neural analysis CPU threading (Phase 1+2 of CPU affinity optimization)
    // This sets OpenMP/OpenVINO thread count to avoid hyperthreading slowdown with VNNI
    let threads_str = config.neural_cpu_threads.to_string();
    std::env::set_var("OMP_NUM_THREADS", &threads_str);
    std::env::set_var("OPENVINO_CPU_THREADS", &threads_str);
    info!(
        "Neural CPU threads configured: {} (OMP_NUM_THREADS, OPENVINO_CPU_THREADS)",
        config.neural_cpu_threads
    );

    // Configure FFmpeg CPU affinity (Phase 3 of CPU affinity optimization)
    // This pins FFmpeg to SMT cores (8-15) while neural analysis uses physical cores (0-7)
    if let Some(ref cores) = config.ffmpeg_cpu_cores {
        std::env::set_var("VCLIP_FFMPEG_CPU_CORES", cores);
        info!(
            "FFmpeg CPU cores configured: {} (will use taskset on Linux)",
            cores
        );
    }

    // Create queue client
    let queue = match JobQueue::from_env() {
        Ok(q) => q,
        Err(e) => {
            error!("Failed to create job queue: {}", e);
            std::process::exit(1);
        }
    };

    // Create executor
    let executor = match JobExecutor::new(config, queue) {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to create job executor: {}", e);
            std::process::exit(1);
        }
    };

    // Setup signal handlers
    let shutdown_handle = tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal");
    });

    // Run executor
    if let Err(e) = executor.run().await {
        error!("Executor error: {}", e);
        std::process::exit(1);
    }

    // Wait for shutdown
    shutdown_handle.await.ok();

    info!("Worker shutdown complete");
}
