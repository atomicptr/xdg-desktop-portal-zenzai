use std::future::pending;

use config::{Config, ConfigErr};
use portals::settings::service::SettingsService;
use zbus::{Result, conn::Builder};

mod config;
mod portals;

#[tokio::main]
async fn main() -> Result<()> {
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

    let mut conn = Builder::session()?.name("org.freedesktop.impl.portal.desktop.porta")?;

    if let Some(config) = config.settings {
        if config.enabled {
            any_enabled = true;

            println!("Settings portal enabled!");
            conn = conn.serve_at(
                "/org/freedesktop/portal/desktop",
                SettingsService { config },
            )?;
        }
    }

    if !any_enabled {
        println!("Nothing was enbaled, quitting");
        return Ok(());
    }

    let _conn = conn.build().await?;
    pending::<()>().await;

    Ok(())
}
