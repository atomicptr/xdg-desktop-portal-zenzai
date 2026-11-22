use anyhow::Result;
use config::Config;
use constants::{APP_VERSION, DBUS_NAME, DBUS_PATH};
use portals::{
    appchooser::service::{AppChooserService, SELECT_COMMAND},
    print::{dialog::DIALOG_COMMAND, service::PrintService},
    settings::service::SettingsService,
};
use reload::{ConfigWatcher, EnabledPortals, Reload};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;
use which::which;
use zbus::conn::Builder;
use zenzai_core::config::load_or_default;

mod config;
mod constants;
mod portals;
mod reload;
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

    let mut watcher = ConfigWatcher::new()?;
    let config: Config = load_or_default()?;
    let mut reload = Reload {
        enabled: EnabledPortals::from(&config),
        appchooser: None,
        print: None,
    };

    tracing::debug!("Config: {:?}", config);

    let _terminal = config.terminal;
    let mut any_enabled = false;

    let mut conn = Builder::session().name(DBUS_NAME);

    if let Some(config) = config.settings
        && config.enabled
    {
        any_enabled = true;

        tracing::info!("portal: org.freedesktop.portal.Settings enabled!");
        conn = conn.serve_at(DBUS_PATH, SettingsService::new(config));
    }

    if let Some(config) = config.appchooser
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(SELECT_COMMAND);
        match which(command) {
            Ok(command) => {
                any_enabled = true;
                tracing::info!("portal: org.freedesktop.portal.AppChooser enabled!");
                let (sender, command) = watch::channel(command);
                reload.appchooser = Some(sender);
                conn = conn.serve_at(DBUS_PATH, AppChooserService { command });
            }
            Err(err) => tracing::error!("AppChooser disabled, could not find {command:?}: {err}"),
        }
    }

    if let Some(config) = config.print
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(DIALOG_COMMAND);

        match which(command) {
            Ok(command) => {
                any_enabled = true;

                tracing::info!("portal: org.freedesktop.portal.Print enabled!");
                let (sender, command) = watch::channel(command);
                reload.print = Some(sender);
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

    let conn = conn.build().await?;
    // Reload once after registration to catch edits made during startup.
    loop {
        if let Err(err) = reload.apply(&conn).await {
            tracing::warn!("config reload rejected, keeping current configuration: {err:#}");
        }

        watcher.changed().await;
    }
}
