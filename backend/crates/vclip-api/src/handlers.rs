//! Request handlers.

pub mod admin;
pub mod health;
pub mod settings;
pub mod videos;

pub use admin::*;
pub use health::*;
pub use settings::*;
pub use videos::*;
