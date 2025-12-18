//! Business logic services.
//!
//! This module contains the core business logic for the ViralClip API:
//!
//! - [`UserService`] - User management, plan limits, storage tracking
//! - [`CreditService`] - Credit reservation, transaction recording, history
//! - [`StaleJobDetector`] - Background job cleanup

pub mod credit;
pub mod stale_job_detector;
pub mod user;

pub use credit::CreditService;
pub use stale_job_detector::StaleJobDetector;
pub use user::UserService;
