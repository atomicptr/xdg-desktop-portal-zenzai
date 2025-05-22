use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppChooserConfig {
    pub enabled: bool,
    pub runner_type: Option<RunnerType>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RunnerType {
    Dmenu(Command),
}

#[derive(Debug, Deserialize)]
pub struct Command {
    pub command: String,
    pub arguments: Vec<String>,
}
