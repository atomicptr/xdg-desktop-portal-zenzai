use std::{env, future::pending};

use config::{Config, ConfigErr};
use constants::{APP_VERSION, DBUS_NAME};
use portals::{
    appchooser::service::AppChooserService, secret::service::SecretService,
    settings::service::SettingsService,
};
use terminal::{command_path, terminal_from_env};
use zbus::{Result, conn::Builder};

mod config;
mod constants;
mod portals;
mod terminal;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    unsafe {
        env::set_var("RUST_LOG", "xdg-desktop-portal-zenzai=info");
    }

    tracing_subscriber::fmt().init();
    tracing::info!("xdg-desktop-portal-zenzai v{} started!", APP_VERSION);

    let config = match Config::from_xdg_dirs() {
        Ok(config) => config,
        Err(ConfigErr::NotFound) => {
            panic!("Could not find config file");
        }
        Err(ConfigErr::IOError(err)) => {
            panic!("Config IO Error: {:?}", err);
        }
        Err(ConfigErr::ParseError(err)) => {
            panic!("Config Parse Error: {:?}", err.message());
        }
    };

    tracing::debug!("Config: {:?}", config);

    let terminal = config.terminal.unwrap_or(terminal_from_env());
    if command_path(&terminal).is_none() {
        panic!("Could not find terminal: {:?}", terminal);
    }

    let mut any_enabled = false;

    let mut conn = Builder::session()?.name(DBUS_NAME)?;

    if let Some(config) = config.settings {
        if config.enabled {
            any_enabled = true;

            tracing::info!("portal: org.freedesktop.portal.Settings enabled!");
            conn = conn.serve_at(
                "/org/freedesktop/portal/desktop",
                SettingsService { config },
            )?;
        }
    }

    if let Some(config) = config.appchooser {
        if config.enabled {
            any_enabled = true;

            tracing::info!("portal: org.freedesktop.portal.AppChooser enabled!");
            conn = conn.serve_at(
                "/org/freedesktop/portal/desktop",
                AppChooserService { terminal, config },
            )?;
        }
    }

    if let Some(config) = config.secret {
        if config.enabled {
            any_enabled = true;

            tracing::info!("portal: org.freedesktop.portal.Secret enabled!");
            conn = conn.serve_at("/org/freedesktop/portal/desktop", SecretService)?;
        }
    }

    if !any_enabled {
        tracing::error!("No portal was enbaled, quitting");
        return Ok(());
    }

    let _conn = conn.build().await?;
    pending::<()>().await;

    Ok(())
}
