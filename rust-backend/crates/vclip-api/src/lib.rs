//! Axum HTTP/WS API server.
//!
//! This crate provides:
//! - Full REST API parity with Python backend
//! - WebSocket endpoints for processing
//! - Firebase ID token verification
//! - Rate limiting and security headers

pub mod auth;
pub mod config;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod ws;

pub use config::ApiConfig;
pub use error::{ApiError, ApiResult};
pub use routes::create_router;
pub use state::AppState;
