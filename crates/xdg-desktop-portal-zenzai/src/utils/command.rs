use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::Result;
use which::which;

const BIN_DIR_ENV: &str = "ZENZAI_BIN_DIR";

pub fn resolve_command(command: &str) -> Result<PathBuf> {
    if let Some(directory) = env::var_os(BIN_DIR_ENV).filter(|value| !value.is_empty())
        && let Some(filename) = Path::new(command).file_name()
    {
        let candidate = env::current_dir()?.join(directory).join(filename);

        if let Ok(path) = which(&candidate) {
            tracing::debug!(command, path = %path.display(), "using helper executable override");
            return Ok(path);
        }
    }

    Ok(which(command)?)
}
