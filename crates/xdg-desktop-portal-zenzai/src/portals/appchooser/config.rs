use std::collections::HashMap;

use serde::Deserialize;
use termlauncher::{Application, CustomTerminal, Terminal};

// for terminals termlauncher does not know, almost all of them accept `-e <command>`
const CUSTOM_RUN_ARG: &str = "-e";
const FALLBACK_TERMINAL: &str = "xterm";

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppChooserConfig {
    pub enabled: bool,
    pub runner: Option<RunnerType>,
    #[serde(default)]
    pub defaults: HashMap<String, DefaultMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum RunnerType {
    Dmenu(Command),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Target {
    Command(Command),
    DesktopFile(String),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum DefaultMapping {
    One(Target),
    Choice(Vec<Target>),
}

impl DefaultMapping {
    pub fn targets(&self) -> &[Target] {
        match self {
            Self::One(target) => std::slice::from_ref(target),
            Self::Choice(targets) => targets,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Command {
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

impl Command {
    pub fn in_terminal(self, terminal: &Terminal) -> termlauncher::Result<Command> {
        let args = self.arguments.iter().map(String::as_str).collect();
        let cmd = terminal.build_command(&Application::new(&self.command).with_args(&args))?;

        Ok(Command {
            command: cmd.get_program().to_string_lossy().into_owned(),
            arguments: cmd
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        })
    }
}

pub fn terminal(name: Option<&str>) -> Terminal {
    let custom = |executable: &str| {
        Terminal::Custom(CustomTerminal {
            executable: executable.to_string(),
            run_arg: Some(CUSTOM_RUN_ARG.to_string()),
            ..Default::default()
        })
    };

    match name {
        Some(name) => Terminal::find_by_name(name).unwrap_or_else(|| custom(name)),
        None => Terminal::find_available().unwrap_or_else(|| custom(FALLBACK_TERMINAL)),
    }
}
