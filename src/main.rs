use std::{env, future::pending};

use config::{Config, ConfigErr};
use constants::{APP_VERSION, DBUS_NAME};
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

    if !any_enabled {
        tracing::error!("No portal was enbaled, quitting");
        return Ok(());
    }

    let _conn = conn.build().await?;
    pending::<()>().await;

    Ok(())
}
