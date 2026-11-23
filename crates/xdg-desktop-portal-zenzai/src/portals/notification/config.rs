use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
