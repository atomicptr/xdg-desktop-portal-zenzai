use std::{path::PathBuf, process::Stdio};

use anyhow::{Context, Result};
use freedesktop_desktop_entry::{DesktopEntry, default_paths};
use termlauncher::Terminal;
use tokio::{io::AsyncWriteExt, process};

use super::config::{Command, Target};

const DESKTOP_SUFFIX: &str = ".desktop";
const NO_LOCALES: &[&str] = &[];

pub struct Launchable {
    pub label: String,
    pub command: Command,
}

// field codes like %f are filled in by the desktop entry, plain commands get the file appended
pub fn resolve(target: &Target, file: &str, terminal: &Terminal) -> Result<Launchable> {
    let id = match target {
        Target::Command(command) => {
            let mut command = command.clone();
            command.arguments.push(file.to_string());

            return Ok(Launchable {
                label: command.command.clone(),
                command,
            });
        }
        Target::DesktopFile(id) => id.trim_end_matches(DESKTOP_SUFFIX),
    };

    let path = desktop_file(id).with_context(|| format!("no desktop file for {id}"))?;
    let entry = DesktopEntry::from_path(path, Some(NO_LOCALES))?;

    let mut exec = entry.parse_exec_with_uris(&[file], NO_LOCALES)?.into_iter();
    let program = exec.next().context("empty Exec")?;

    let command = Command {
        command: program,
        arguments: exec.collect(),
    };

    let command = match entry.terminal() {
        true => command.in_terminal(terminal)?,
        false => command,
    };

    Ok(Launchable {
        label: entry
            .name(NO_LOCALES)
            .map_or(id.to_string(), |name| name.into_owned()),
        command,
    })
}

fn desktop_file(id: &str) -> Option<PathBuf> {
    default_paths()
        .map(|dir| dir.join(format!("{id}{DESKTOP_SUFFIX}")))
        .find(|path| path.is_file())
}

// returns None when the user dismissed the picker or entered something unknown
pub async fn pick<'a>(runner: &Command, items: &'a [Launchable]) -> Result<Option<&'a Launchable>> {
    if let [only] = items {
        return Ok(Some(only));
    }

    let mut child = process::Command::new(&runner.command)
        .args(&runner.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
    let mut stdin = child.stdin.take().context("could not open picker stdin")?;
    stdin.write_all(labels.join("\n").as_bytes()).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    let choice = String::from_utf8_lossy(&output.stdout);

    Ok(items.iter().find(|item| item.label == choice.trim()))
}

pub fn spawn(command: &Command) -> Result<()> {
    tracing::info!("launching {command:?}");
    process::Command::new(&command.command)
        .args(&command.arguments)
        .spawn()?;

    Ok(())
}
