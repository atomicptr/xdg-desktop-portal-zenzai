use std::{env, fs, path::PathBuf};

use serde::Deserialize;

use crate::{
    constants::CONFIG_APP_NAME,
    portals::{secret::config::SecretConfig, settings::config::SettingsConfig},
};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub settings: Option<SettingsConfig>,
    pub secret: Option<SecretConfig>,
}

pub enum ConfigErr {
    NotFound,
    IOError(std::io::Error),
    ParseError(toml::de::Error),
}

impl From<std::io::Error> for ConfigErr {
    fn from(value: std::io::Error) -> Self {
        ConfigErr::IOError(value)
    }
}

impl From<toml::de::Error> for ConfigErr {
    fn from(value: toml::de::Error) -> Self {
        ConfigErr::ParseError(value)
    }
}

impl Config {
    pub fn from_xdg_dirs() -> Result<Config, ConfigErr> {
        let config_home = env::var("XDG_CONFIG_HOME")
            .map(|path| path.into())
            .or_else(|_| {
                let home_dir: PathBuf = env::var("HOME").expect("cant evaluate HOME").into();
                Ok::<PathBuf, ConfigErr>(home_dir.join(".config"))
            })?
            .join(CONFIG_APP_NAME)
            .join("config.toml");

        Config::from_path(config_home)
    }

    pub fn from_path(path: PathBuf) -> Result<Config, ConfigErr> {
        if !path.exists() {
            return Err(ConfigErr::NotFound);
        }

        let data = fs::read_to_string(path)?;

        let config: Config = toml::from_str(data.as_str())?;

        Ok(config)
    }
}
