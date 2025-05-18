use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SecretConfig {
    pub enabled: bool,
}
