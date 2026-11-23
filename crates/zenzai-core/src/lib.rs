#[cfg(feature = "ui")]
rust_i18n::i18n!("../../locales", fallback = "en");

pub mod appchooser;
pub mod appearance;
pub mod config;
#[cfg(feature = "ui")]
mod error;
#[cfg(feature = "ui")]
pub mod i18n;
pub mod ipc;
#[cfg(feature = "ui")]
pub use error::show_error;

#[cfg(feature = "ui")]
pub mod ui;
