//! Business logic services.
//!
//! This module contains the core business logic for the ViralClip API:
//!
//! - [`UserService`] - User management, plan limits, storage tracking
//! - [`CreditService`] - Credit reservation, transaction recording, history
//! - [`StaleJobDetector`] - Background job cleanup
//! - [`gemini`] - Gemini AI client for scene generation

pub mod credit;
pub mod gemini;
pub mod stale_job_detector;
pub mod user;

pub use credit::CreditService;
pub use gemini::GeminiClient;
pub use stale_job_detector::StaleJobDetector;
pub use user::UserService;
