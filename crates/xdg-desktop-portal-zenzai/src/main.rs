use std::future::pending;

use anyhow::Result;
use config::{Config, TerminalName};
use constants::{APP_VERSION, DBUS_NAME, DBUS_PATH};
use portals::{
    appchooser::{config::terminal, service::AppChooserService},
    print::{dialog::DIALOG_COMMAND, service::PrintService},
    settings::service::SettingsService,
};
use tracing_subscriber::EnvFilter;
use which::which;
use zbus::conn::Builder;
use zenzai_core::config::load_or_default;

mod config;
mod constants;
mod portals;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    // targets are module paths, they use the crate name with underscores
    let level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(format!(
            "{}={level}",
            env!("CARGO_CRATE_NAME")
        )))
        .init();

    tracing::info!("xdg-desktop-portal-zenzai v{} started!", APP_VERSION);

    let config: Config = load_or_default()?;

    tracing::debug!("Config: {:?}", config);

    let config_terminal = config.terminal;
    let mut any_enabled = false;

    let mut conn = Builder::session().name(DBUS_NAME);

    if let Some(config) = config.settings
        && config.enabled
    {
        any_enabled = true;

        tracing::info!("portal: org.freedesktop.portal.Settings enabled!");
        conn = conn.serve_at(DBUS_PATH, SettingsService { config });
    }

    if let Some(config) = config.appchooser
        && config.enabled
    {
        any_enabled = true;

        let terminal = terminal(config_terminal.as_ref().map(TerminalName::as_str));

        tracing::info!("portal: org.freedesktop.portal.AppChooser enabled!");
        conn = conn.serve_at(DBUS_PATH, AppChooserService { terminal, config });
    }

    if let Some(config) = config.print
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(DIALOG_COMMAND);

        match which(command) {
            Ok(command) => {
                any_enabled = true;

                tracing::info!("portal: org.freedesktop.portal.Print enabled!");
                conn = conn.serve_at(DBUS_PATH, PrintService::new(command));
            }
            Err(_) => tracing::error!(
                "portal: org.freedesktop.portal.Print disabled, could not find print dialog {command:?}"
            ),
        }
    }

    if !any_enabled {
        tracing::error!("No portal was enabled, quitting");
        return Ok(());
    }

    let _conn = conn.build().await?;
    pending::<()>().await;

    Ok(())
}
