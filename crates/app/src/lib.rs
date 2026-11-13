pub mod application;
pub mod error;
pub mod state;

#[cfg(feature = "desktop")]
pub mod commands;
#[cfg(feature = "desktop")]
pub mod desktop;

pub use error::{AppError, AppResult};
