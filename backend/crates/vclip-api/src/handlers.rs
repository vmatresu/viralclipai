//! Request handlers.

pub mod admin;
pub mod analysis;
pub mod clip_delivery;
pub mod health;
pub mod jobs;
pub mod settings;
pub mod storage;
pub mod video_status;
pub mod videos;

pub use admin::*;
pub use analysis::*;
pub use clip_delivery::*;
pub use health::*;
pub use jobs::*;
pub use settings::*;
pub use storage::*;
pub use video_status::*;
pub use videos::*;
