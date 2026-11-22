use serde::{Deserialize, Serialize};
use zenzai_core::{
    config::{load_state, save_state},
    ipc::PrintSettings,
};

use crate::settings::{JobSettings, key};

const STATE_FILE: &str = "print-dialog.toml";

// page selection, copies and orientation are per document
const REMEMBERED_KEYS: [&str; 7] = [
    key::PAPER_FORMAT,
    key::DUPLEX,
    key::USE_COLOR,
    key::QUALITY,
    key::NUMBER_UP,
    key::NUMBER_UP_LAYOUT,
    key::COLLATE,
];

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Remembered {
    pub printer: Option<String>,
    pub settings: PrintSettings,
}

impl Remembered {
    pub fn load() -> Self {
        load_state(STATE_FILE)
    }

    // Save as PDF has no printer, keep the previous one for the next real print
    pub fn save(printer: Option<&str>, settings: &JobSettings) {
        let previous = Self::load();

        let remembered = Self {
            printer: printer.map(str::to_string).or(previous.printer),
            settings: settings
                .to_portal(None, None)
                .into_iter()
                .filter(|(key, _)| REMEMBERED_KEYS.contains(&key.as_str()))
                .collect(),
        };

        if let Err(err) = save_state(STATE_FILE, &remembered) {
            tracing::warn!("could not remember print settings: {err:#}");
        }
    }
}
