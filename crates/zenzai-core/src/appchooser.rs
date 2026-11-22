use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppChooserConfig {
    #[serde(default)]
    pub enabled: bool,
    pub command: Option<String>,
    #[serde(default)]
    pub defaults: BTreeMap<String, DefaultMapping>,
}

impl AppChooserConfig {
    pub fn mapping(&self, content_type: &str) -> Option<&DefaultMapping> {
        self.defaults.get(content_type).or_else(|| {
            self.defaults
                .iter()
                .filter(|(pattern, _)| {
                    pattern
                        .strip_suffix('*')
                        .is_some_and(|prefix| content_type.starts_with(prefix))
                })
                .max_by_key(|(pattern, _)| pattern.len())
                .map(|(_, mapping)| mapping)
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Command(Command),
    DesktopFile(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DefaultMapping {
    One(Target),
    Choice(Vec<Target>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Command {
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

// Retain the terminal format used before 0.4.0.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TerminalName {
    Name(String),
    Custom { custom: String },
}

impl TerminalName {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Name(name) | Self::Custom { custom: name } => name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_longest_configured_patterns_win() {
        let config: AppChooserConfig = toml::from_str(
            r#"
            [defaults]
            "*" = "fallback"
            "image/*" = ["viewer", { command = "imv" }]
            "image/png" = "png-viewer"
        "#,
        )
        .unwrap();

        assert!(
            matches!(config.mapping("image/png"), Some(DefaultMapping::One(Target::DesktopFile(id))) if id == "png-viewer")
        );
        assert!(
            matches!(config.mapping("image/jpeg"), Some(DefaultMapping::Choice(targets)) if targets.len() == 2)
        );
        assert!(
            matches!(config.mapping("text/plain"), Some(DefaultMapping::One(Target::DesktopFile(id))) if id == "fallback")
        );
        assert!(AppChooserConfig::default().mapping("image/png").is_none());
    }
}
