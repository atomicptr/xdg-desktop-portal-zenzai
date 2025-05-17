use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::portals::settings::config::SettingsConf;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub settings: Option<SettingsConf>,
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
    pub fn from_path(path: PathBuf) -> Result<Config, ConfigErr> {
        if !path.exists() {
            return Err(ConfigErr::NotFound);
        }

        let data = fs::read_to_string(path)?;

        let config: Config = toml::from_str(data.as_str())?;

        Ok(config)
    }
}
