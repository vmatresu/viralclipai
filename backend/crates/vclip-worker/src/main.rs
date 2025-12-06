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

    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::from_default_env().add_directive("vclip=info".parse().unwrap()))
        .init();

    info!("Starting vclip-worker");

    // Load configuration
    let config = WorkerConfig::from_env();
    info!("Worker config: {:?}", config);

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
