//! Business logic services.

pub mod stale_job_detector;
pub mod user;

pub use stale_job_detector::StaleJobDetector;
pub use user::UserService;
