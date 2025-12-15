//! Axum API server binary.

use std::net::SocketAddr;

use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use vclip_api::{create_router, metrics, ApiConfig, AppState, StaleJobDetector};

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Install rustls crypto provider (required for rustls 0.23+)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Initialize tracing with colored output for dev, JSON for production
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v.to_lowercase() == "json")
        .unwrap_or(false);

    let env_filter = EnvFilter::from_default_env()
        .add_directive("vclip=info".parse().unwrap());

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

    info!("Starting vclip-api");

    // Load configuration
    let config = ApiConfig::from_env();
    info!("API config: host={}, port={}", config.host, config.port);

    // Create application state
    let state = match AppState::new(config.clone()).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to create application state: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize metrics
    let metrics_enabled = std::env::var("METRICS_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    let metrics_handle = if metrics_enabled {
        info!("Prometheus metrics enabled at /metrics");
        Some(metrics::init_metrics())
    } else {
        None
    };

    // Start stale job detector background task
    let stale_detector = StaleJobDetector::new(
        std::sync::Arc::clone(&state.progress),
        std::sync::Arc::clone(&state.firestore),
    );
    tokio::spawn(async move {
        stale_detector.run().await;
    });

    // Create router
    let app = create_router(state, metrics_handle);

    // Bind and serve
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid bind address");

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    info!("Server shutdown complete");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Received shutdown signal");
}
