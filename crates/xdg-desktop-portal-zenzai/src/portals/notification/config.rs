use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct NotificationConfig {
    pub enabled: bool,
}
