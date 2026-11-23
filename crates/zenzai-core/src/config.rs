use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};

const APP_NAME: &str = "xdg-desktop-portal-zenzai";
const CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_CONFIG_DIRS: &str = "/etc/xdg";

// User config takes precedence over $XDG_CONFIG_DIRS.
#[must_use]
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

// Missing config uses defaults; invalid config is an error.
pub fn load_or_default<T: DeserializeOwned + Default>() -> Result<T> {
    let Some(path) = config_paths().into_iter().find(|path| path.is_file()) else {
        return Ok(T::default());
    };

    let data = fs::read_to_string(&path)?;
    parse_config(&data, &path)
}

fn parse_config<T: DeserializeOwned>(data: &str, path: &Path) -> Result<T> {
    toml::from_str(data).map_err(|error| {
        let location = error
            .span()
            .map(|span| format!(" at byte offset {}", span.start))
            .unwrap_or_default();

        // Both source excerpts and deserialization messages can contain personal data.
        anyhow!("could not parse {}{location}", path.display())
    })
}

fn state_file(name: &str) -> Option<PathBuf> {
    dirs::state_dir().map(|dir| dir.join(APP_NAME).join(name))
}

// Unreadable state falls back to defaults.
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Config {
        #[serde(rename = "enabled")]
        _enabled: bool,
    }

    #[test]
    fn parse_errors_do_not_expose_config_values() {
        const SENTINEL: &str = "PRIVATE_ACCOUNT_SENTINEL";
        let path = Path::new("/config/config.toml");

        for data in [
            format!("enabled = {SENTINEL}"),
            format!("enabled = \"{SENTINEL}\""),
        ] {
            let error = parse_config::<Config>(&data, path).unwrap_err();

            for rendered in [
                error.to_string(),
                format!("{error:#}"),
                format!("{error:?}"),
                format!("{error:#?}"),
            ] {
                assert!(!rendered.contains(SENTINEL));
                assert!(rendered.contains("/config/config.toml at byte offset "));
            }

            assert_eq!(error.chain().count(), 1);

            for cause in error.chain() {
                assert!(!cause.to_string().contains(SENTINEL));
                assert!(!format!("{cause:?}").contains(SENTINEL));
            }
        }
    }
}
