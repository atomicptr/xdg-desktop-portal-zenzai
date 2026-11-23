use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PrintConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

impl Default for PrintConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: None,
        }
    }
}
