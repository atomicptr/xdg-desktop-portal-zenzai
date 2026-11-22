use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

const APP_NAME: &str = "xdg-desktop-portal-zenzai";
const CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_CONFIG_DIRS: &str = "/etc/xdg";

// $XDG_CONFIG_HOME first, then $XDG_CONFIG_DIRS where e.g. the NixOS module puts its file
pub fn config_paths() -> Vec<PathBuf> {
    let config_dirs = env::var("XDG_CONFIG_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| DEFAULT_CONFIG_DIRS.to_string());

    dirs::config_dir()
        .into_iter()
        .chain(env::split_paths(&config_dirs).filter(|dir| dir.is_absolute()))
        .map(|dir| dir.join(APP_NAME).join(CONFIG_FILE_NAME))
        .collect()
}

fn config_file() -> Option<PathBuf> {
    config_paths().into_iter().find(|path| path.is_file())
}

// a missing file means defaults, a broken one is an error
pub fn load_or_default<T: DeserializeOwned + Default>() -> Result<T> {
    let Some(path) = config_file() else {
        return Ok(T::default());
    };

    let data = fs::read_to_string(&path)?;
    toml::from_str(&data).with_context(|| format!("could not parse {}", path.display()))
}

fn state_file(name: &str) -> Option<PathBuf> {
    dirs::state_dir().map(|dir| dir.join(APP_NAME).join(name))
}

// state is only a convenience, unreadable state falls back to defaults
pub fn load_state<T: DeserializeOwned + Default>(name: &str) -> T {
    state_file(name)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|data| toml::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_state<T: Serialize>(name: &str, value: &T) -> Result<()> {
    let path = state_file(name).context("could not determine state directory")?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    fs::write(path, toml::to_string(value)?)?;
    Ok(())
}
