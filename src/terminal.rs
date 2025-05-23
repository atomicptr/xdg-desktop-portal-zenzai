use std::{path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use which::which;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum Terminal {
    Alacritty,
    Foot,
    Ghostty,
    Kitty,
    Xterm,
    Custom(String),
}

pub fn command_name(term: &Terminal) -> Option<String> {
    match term {
        Terminal::Alacritty => String::from_str("alacritty").map_or(None, Some),
        Terminal::Ghostty => String::from_str("ghostty").map_or(None, Some),
        Terminal::Kitty => String::from_str("kitty").map_or(None, Some),
        Terminal::Xterm => String::from_str("xterm").map_or(None, Some),
        Terminal::Foot => String::from_str("foot").map_or(None, Some),
        Terminal::Custom(term) => which(term).map(|_| Some(term.clone())).unwrap_or(None),
    }
}

pub fn command_path(term: &Terminal) -> Option<PathBuf> {
    let name = command_name(term)?;
    which(name).map_or(None, Some)
}

pub fn terminal_from_env() -> Terminal {
    if let Ok(_) = which("ghostty") {
        return Terminal::Ghostty;
    }

    if let Ok(_) = which("kitty") {
        return Terminal::Kitty;
    }

    if let Ok(_) = which("alacritty") {
        return Terminal::Alacritty;
    }

    if let Ok(_) = which("foot") {
        return Terminal::Foot;
    }

    return Terminal::Xterm;
}
