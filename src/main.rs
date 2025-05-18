use std::{env, future::pending};

use config::{Config, ConfigErr};
use constants::DBUS_NAME;
use portals::settings::service::SettingsService;
use zbus::{Result, conn::Builder};

mod config;
mod constants;
mod portals;

#[tokio::main]
async fn main() -> Result<()> {
    unsafe {
        env::set_var("RUST_LOG", "xdg-desktop-portal-zenzai=info");
    }

    tracing_subscriber::fmt().init();
    tracing::info!("xdg-desktop-portal-zenzai started!");

    let config = match Config::from_xdg_dirs() {
        Ok(config) => config,
        Err(ConfigErr::NotFound) => {
            panic!("Could not find config file");
        }
        Err(ConfigErr::IOError(err)) => {
            panic!("IO Error: {:?}", err);
        }
        Err(ConfigErr::ParseError(err)) => {
            panic!("Parse Error: {:?}", err.message());
        }
    };

    let mut any_enabled = false;

    let mut conn = Builder::session()?.name(DBUS_NAME)?;

    if let Some(config) = config.settings {
        if config.enabled {
            any_enabled = true;

            tracing::info!("Settings portal enabled!");
            conn = conn.serve_at(
                "/org/freedesktop/portal/desktop",
                SettingsService { config },
            )?;
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
