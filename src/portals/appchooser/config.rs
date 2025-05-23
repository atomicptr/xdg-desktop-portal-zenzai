use std::collections::HashMap;

use serde::Deserialize;

use crate::terminal::{Terminal, command_path};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppChooserConfig {
    pub enabled: bool,
    pub runner: Option<RunnerType>,
    pub defaults: HashMap<String, DefaultMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum RunnerType {
    Dmenu(Command),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DefaultMapping {
    Command(Command),
    CommandChoice(Vec<Command>),
    DesktopFile(String),
    DesktopFileChoice(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
pub struct Command {
    pub command: String,
    pub arguments: Option<Vec<String>>,
}

impl Command {
    pub fn with_terminal(&self, terminal: &Terminal) -> Command {
        let command = command_path(&terminal)
            .unwrap()
            .to_str()
            .expect("could not convert OsStr to &str")
            .to_string();

        let mut arguments = Vec::new();
        arguments.push("-e".to_string());
        arguments.push(self.command.clone());
        self.arguments
            .clone()
            .unwrap_or_default()
            .iter()
            .for_each(|arg| arguments.push(arg.clone()));

        Command {
            command,
            arguments: Some(arguments),
        }
    }

    pub fn with_input_file(&self, file: String) -> Command {
        // TODO: add support for other ways to supply file paths
        let mut args = self.arguments.clone().unwrap_or_default();
        args.push(file);

        Command {
            command: self.command.clone(),
            arguments: Some(args),
        }
    }
}
