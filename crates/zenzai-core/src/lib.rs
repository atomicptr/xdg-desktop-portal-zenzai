pub mod appchooser;
pub mod appearance;
pub mod config;
#[cfg(feature = "ui")]
mod error;
pub mod ipc;
#[cfg(feature = "ui")]
pub use error::show_error;

#[cfg(feature = "ui")]
pub mod ui;
