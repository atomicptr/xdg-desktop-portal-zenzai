use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct PrintConfig {
    pub enabled: bool,
    pub command: Option<String>,
}
