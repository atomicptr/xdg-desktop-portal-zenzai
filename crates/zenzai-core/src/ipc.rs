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

// Independent protocol flags must retain their serialized boolean representation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PrintRequest {
    pub title: String,
    pub settings: PrintSettings,
    pub page_setup: PageSetup,
    pub document: Option<PathBuf>,
    pub interactive: bool,
    // Page selection, order, and scaling are already applied.
    pub prerendered: bool,
    pub accept_label: Option<String>,
    pub has_current_page: bool,
    pub has_selected_pages: bool,
    pub supported_output_formats: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintAction {
    #[default]
    Submit,
    Preview,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PrintResponse {
    pub settings: PrintSettings,
    pub page_setup: PageSetup,
    #[serde(default)]
    pub action: PrintAction,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppSelectRequest {
    pub uri: String,
    pub content_type: String,
    pub choices: Vec<String>,
    pub activation_token: Option<String>,
    pub email: Option<Email>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Email {
    pub addresses: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<String>,
}

pub const MAX_LAUNCHER_ICON_BYTES: usize = 16 * 1024 * 1024;
// JSON byte arrays expand up to fourfold; allow headroom for the remaining fields.
pub const MAX_LAUNCHER_MESSAGE_BYTES: u64 = 80 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LauncherType {
    Application,
    Webapp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherRequest {
    pub app_id: String,
    pub name: String,
    pub launcher_type: LauncherType,
    pub target: Option<String>,
    pub icon: Vec<u8>,
    pub editable_name: bool,
    #[serde(default)]
    pub editable_icon: bool,
    pub parent_window: String,
    pub modal: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LauncherResponse {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Vec<u8>>,
}

// Access dialogs contain text only, including serialized choices.
pub const MAX_ACCESS_MESSAGE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub app_id: String,
    pub parent_window: String,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub deny_label: Option<String>,
    pub grant_label: Option<String>,
    pub icon: Option<String>,
    pub modal: bool,
    pub choices: Vec<AccessChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessChoice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub selected: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessResponse {
    pub choices: Vec<(String, String)>,
}
