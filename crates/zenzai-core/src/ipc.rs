use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_CANCELLED: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

pub type PrintSettings = BTreeMap<String, String>;

pub type PageSetup = BTreeMap<String, SetupValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SetupValue {
    Number(f64),
    Text(String),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PrintRequest {
    pub title: String,
    pub settings: PrintSettings,
    pub page_setup: PageSetup,
    pub document: Option<PathBuf>,
    pub interactive: bool,
    // the application already applied page selection, order and scaling,
    // the dialog must not apply them again
    pub prerendered: bool,
    pub accept_label: Option<String>,
    pub has_current_page: bool,
    pub has_selected_pages: bool,
    pub supported_output_formats: Option<Vec<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PrintResponse {
    pub settings: PrintSettings,
    pub page_setup: PageSetup,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSelectRequest {
    pub uri: String,
    pub content_type: String,
    pub choices: Vec<String>,
}
