use std::{env, fs, path::PathBuf};

use ini::Ini;

use crate::terminal::Terminal;

use super::config::Command;

#[derive(Debug)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: Command,
    pub is_terminal: bool,
}

impl DesktopEntry {
    pub fn command(&self, terminal: &Terminal) -> Command {
        if self.is_terminal {
            self.exec.with_terminal(terminal)
        } else {
            self.exec.clone()
        }
    }
}

fn desktop_files() -> Vec<PathBuf> {
    let xdg_data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share".to_string());

    xdg_data_dirs
        .split(':')
        .filter_map(|s| {
            let applications_dir = PathBuf::from(s).join("applications");

            if applications_dir.is_dir() {
                Some(applications_dir)
            } else {
                None
            }
        })
        .flat_map(|dir| {
            fs::read_dir(&dir).into_iter().flat_map(|read_dir_result| {
                read_dir_result
                    .into_iter()
                    .filter_map(|entry_result| entry_result.ok())
                    .filter_map(|entry| {
                        let path = entry.path();
                        if path.is_file() {
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .filter(|name_str| name_str.ends_with(".desktop"))
                                .map(|_| path.clone())
                        } else {
                            None
                        }
                    })
            })
        })
        .collect()
}

pub fn find_desktop_entry(name: &str) -> Option<DesktopEntry> {
    let name = name.trim_end_matches(".desktop");

    let matching: Vec<PathBuf> = desktop_files()
        .into_iter()
        .filter(|f| f.exists() && f.is_file())
        .filter(|f| {
            f.file_stem()
                .map(|s| s.to_str().unwrap_or("") == name)
                .unwrap_or(false)
        })
        .collect();

    if matching.len() == 0 {
        return None;
    }

    let first = matching.get(0).unwrap();

    let (name, terminal, exec) = Ini::load_from_file(first)
        .map(|conf| {
            conf.section(Some("Desktop Entry"))
                .map(|section| {
                    (
                        section.get("Name").map(|s| s.to_string()),
                        section.get("Terminal").map(|s| {
                            match s.to_string().to_lowercase().as_str() {
                                "true" => true,
                                "1" => true,
                                _ => false,
                            }
                        }),
                        section.get("Exec").map(|s| s.to_string()),
                    )
                })
                .unwrap_or((None, None, None))
        })
        .unwrap_or((None, None, None));

    if exec.is_none() {
        tracing::error!("Entry: {:?} is invalid (no Exec)", first);
        return None;
    }

    let exec = exec.unwrap();
    let name = name.unwrap_or(exec.clone());
    let is_terminal = terminal.unwrap_or(false);

    let exec_parts: Vec<&str> = exec.split(" ").collect();

    let rest = Vec::from(&exec_parts[1..]);

    let exec = Command {
        command: exec_parts.first().unwrap().to_string(),
        arguments: Some(
            rest.iter()
                .filter(|s| !s.starts_with("@@") && !s.starts_with("%"))
                .map(|s| s.to_string())
                .collect(),
        ),
    };

    Some(DesktopEntry {
        name,
        exec,
        is_terminal,
    })
}
