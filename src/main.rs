use std::future::pending;

use config::{Config, ConfigErr};
use portals::settings::service::SettingsService;
use zbus::{Connection, Result, conn::Builder};

mod config;
mod portals;

#[tokio::main]
async fn main() -> Result<()> {
    let config = match Config::from_path("./demo-config.toml".into()) {
        Ok(config) => config,
        Err(ConfigErr::NotFound) => {
            println!("Could not find config file");
            Config::default()
        }
        Err(ConfigErr::IOError(err)) => {
            panic!("IO Error: {:?}", err);
        }
        Err(ConfigErr::ParseError(err)) => {
            panic!("Parse Error: {:?}", err.message());
        }
    };

    println!("Hello, world! {:?}", config);

    let mut conn = Builder::session()?.name("org.freedesktop.portal.desktop.porta")?;

    if let Some(config) = config.settings {
        if config.enabled {
            println!("Settings portal enabled!");
            conn = conn.serve_at(
                "/org/freedesktop/portal/desktop",
                SettingsService { config },
            )?;
        }
    }

    let _conn = conn.build().await?;
    pending::<()>().await;

    Ok(())
}
