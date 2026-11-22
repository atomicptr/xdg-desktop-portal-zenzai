use std::{
    borrow::Cow,
    collections::HashSet,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use freedesktop_desktop_entry::{
    DesktopEntry, current_desktop, default_paths, get_languages_from_env,
};
use termlauncher::{Application, CustomTerminal, Terminal};
use url::Url;
use which::which;
use zenzai_core::{appchooser::Target, ipc::AppSelectRequest};

use crate::email;

const DESKTOP_SUFFIX: &str = ".desktop";
const FILE_PLACEHOLDER: &str = "%file%";
const ICON_SIZE: u16 = 24;

pub struct App {
    pub id: Option<String>,
    pub label: String,
    pub icon: Option<PathBuf>,
    pub menu_visible: bool,
    pub content_types: Vec<String>,
    command: Vec<String>,
    directory: Option<PathBuf>,
    terminal: bool,
}

impl App {
    pub fn matches_id(&self, id: &str) -> bool {
        self.id
            .as_deref()
            .is_some_and(|candidate| desktop_id(candidate) == desktop_id(id))
    }

    pub fn launch(&self, terminal: &Terminal, request: &AppSelectRequest) -> Result<()> {
        let email_command = request.email.as_ref().and_then(|email| {
            self.id
                .as_deref()
                .filter(|id| id.to_ascii_lowercase().contains("thunderbird"))
                .map(|_| email::thunderbird_command(&self.command, &request.uri, email))
        });
        let (program, arguments) = email_command
            .as_ref()
            .unwrap_or(&self.command)
            .split_first()
            .context("empty application command")?;
        let mut command = if self.terminal {
            terminal.build_command(
                &Application::new(program)
                    .with_args(&arguments.iter().map(String::as_str).collect()),
            )?
        } else {
            let mut command = Command::new(program);
            command.args(arguments);
            command
        };

        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }

        if let Some(token) = &request.activation_token {
            command.env("XDG_ACTIVATION_TOKEN", token);
        }

        // Keep application output out of callers' captured chooser output.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .with_context(|| format!("could not launch {}", self.label))?;
        Ok(())
    }
}

pub fn terminal(name: Option<&str>) -> Terminal {
    let custom = |name: &str| {
        Terminal::Custom(CustomTerminal {
            executable: name.to_string(),
            run_arg: Some("-e".to_string()),
            ..Default::default()
        })
    };

    match name {
        Some(name) => Terminal::find_by_name(name).unwrap_or_else(|| custom(name)),
        None => Terminal::find_available().unwrap_or_else(|| custom("xterm")),
    }
}

fn desktop_id(id: &str) -> &str {
    id.strip_suffix(DESKTOP_SUFFIX).unwrap_or(id)
}

pub fn resolve(target: &Target, apps: &mut Vec<App>, file: &str) -> Result<usize> {
    match target {
        Target::DesktopFile(id) => apps
            .iter()
            .position(|app| app.matches_id(id))
            .with_context(|| format!("no launchable desktop entry for {id}")),
        Target::Command(command) => {
            which(&command.command)?;
            let mut arguments = vec![command.command.clone()];
            arguments.extend(expand_arguments(&command.arguments, file));

            apps.push(App {
                id: None,
                label: command.command.clone(),
                icon: None,
                menu_visible: false,
                content_types: Vec::new(),
                command: arguments,
                directory: None,
                terminal: false,
            });

            Ok(apps.len() - 1)
        }
    }
}

fn expand_arguments(arguments: &[String], file: &str) -> Vec<String> {
    let mut expanded: Vec<_> = arguments
        .iter()
        .map(|argument| argument.replace(FILE_PLACEHOLDER, file))
        .collect();

    if !arguments
        .iter()
        .any(|argument| argument.contains(FILE_PLACEHOLDER))
    {
        expanded.push(file.to_string());
    }

    expanded
}

pub fn discover(file: &str) -> Vec<App> {
    let locales = get_languages_from_env();
    let desktops = current_desktop().unwrap_or_default();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    // Walk each XDG root separately so a nested system entry cannot override a user entry.
    for root in default_paths() {
        for path in desktop_files(root.clone()) {
            let Ok(relative) = path.strip_prefix(&root) else {
                continue;
            };
            let id = relative.to_string_lossy().replace('/', "-");
            if !seen.insert(id.clone()) {
                continue;
            }

            let Ok(entry) = DesktopEntry::from_path(&path, Some(&locales)) else {
                continue;
            };

            if let Ok(app) = from_entry(entry, id, file, &locales, &desktops) {
                apps.push(app);
            }
        }
    }

    apps.sort_by_cached_key(|app| (app.label.to_lowercase(), app.id.clone()));
    apps
}

fn from_entry(
    entry: DesktopEntry,
    id: String,
    file: &str,
    locales: &[String],
    desktops: &[String],
) -> Result<App> {
    ensure!(
        entry.type_() == Some("Application") && !entry.hidden(),
        "hidden or non-application entry"
    );
    let matches = |values: Vec<&str>| {
        values.iter().any(|value| {
            desktops
                .iter()
                .any(|desktop| desktop.eq_ignore_ascii_case(value))
        })
    };
    let menu_visible = !entry.no_display()
        && entry.only_show_in().is_none_or(matches)
        && !entry.not_show_in().is_some_and(matches);
    if let Some(executable) = entry.try_exec() {
        which(executable)?;
    }

    // Exec parsing treats field codes literally: %f/%F need local paths, %u/%U URIs.
    let local = Url::parse(file)
        .ok()
        .and_then(|uri| uri.to_file_path().ok());
    let argument = if entry
        .exec()
        .is_some_and(|exec| exec.contains("%f") || exec.contains("%F"))
    {
        local
            .as_ref()
            .context("application requires a local file")?
            .to_string_lossy()
    } else {
        Cow::Borrowed(file)
    };
    let command = entry.parse_exec_with_uris(&[&argument], locales)?;
    which(command.first().context("empty Exec")?)?;
    let icon = entry.icon().and_then(|icon| {
        let path = PathBuf::from(icon);
        if path.is_absolute() {
            return path.is_file().then_some(path);
        }

        freedesktop_icons::lookup(icon).with_size(ICON_SIZE).find()
    });

    Ok(App {
        id: Some(id),
        label: entry
            .name(locales)
            .map_or_else(|| entry.appid.clone(), |name| name.into_owned()),
        icon,
        menu_visible,
        content_types: entry
            .mime_type()
            .unwrap_or_default()
            .into_iter()
            .map(str::to_string)
            .collect(),
        command,
        directory: entry
            .path()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from),
        terminal: entry.terminal(),
    })
}

fn desktop_files(root: PathBuf) -> Vec<PathBuf> {
    let mut pending = vec![root];
    let mut visited = HashSet::new();
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(canonical) = directory.canonicalize() else {
            continue;
        };
        if !visited.insert(canonical) {
            continue;
        }

        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "desktop") {
                files.push(path);
            }
        }
    }

    files
}

pub fn compatible_types(content_type: &str) -> HashSet<String> {
    let mut relations = Vec::new();
    for root in default_paths() {
        let Some(data) = root.parent() else { continue };
        for name in ["aliases", "subclasses"] {
            let Ok(contents) = fs::read_to_string(data.join("mime").join(name)) else {
                continue;
            };
            for line in contents.lines() {
                let mut fields = line.split_whitespace();
                if let (Some(child), Some(parent)) = (fields.next(), fields.next()) {
                    relations.push((child.to_string(), parent.to_string()));
                    if name == "aliases" {
                        relations.push((parent.to_string(), child.to_string()));
                    }
                }
            }
        }
    }

    expand_types(content_type, &relations)
}

fn expand_types(content_type: &str, relations: &[(String, String)]) -> HashSet<String> {
    let mut types = HashSet::from([content_type.to_string()]);
    loop {
        let previous = types.len();
        for (child, parent) in relations {
            if types.contains(child) {
                types.insert(parent.clone());
            }
        }

        if types.len() == previous {
            return types;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_parents_are_transitive_and_cycles_terminate() {
        let relations = [
            ("text/x-rust", "text/plain"),
            ("text/plain", "application/octet-stream"),
            ("text/x-rust", "text/rust"),
            ("text/rust", "text/x-rust"),
            ("image/png", "application/octet-stream"),
        ]
        .map(|(child, parent)| (child.to_string(), parent.to_string()));

        let types = expand_types("text/x-rust", &relations);
        assert_eq!(types.len(), 4);
        assert!(types.contains("text/plain"));
        assert!(types.contains("application/octet-stream"));
        assert!(!types.contains("image/png"));
    }

    #[test]
    fn custom_arguments_place_the_file_without_reinterpreting_it() {
        let file = r#"/tmp/spaced "quoted" 日本語 %file% $(echo nope).txt"#;
        let embedded = format!("--input={file}");
        let repeated = format!("{file}:{file}");
        let cases = [
            (vec![], vec![file]),
            (vec!["--flag"], vec!["--flag", file]),
            (vec!["%file%", "--flag"], vec![file, "--flag"]),
            (
                vec!["--before", "%file%", "--after"],
                vec!["--before", file, "--after"],
            ),
            (vec!["--flag", "%file%"], vec!["--flag", file]),
            (vec!["--input=%file%"], vec![embedded.as_str()]),
            (
                vec!["%file%", "%file%:%file%"],
                vec![file, repeated.as_str()],
            ),
            (vec!["%f", "{{ file }}"], vec!["%f", "{{ file }}", file]),
        ];

        for (arguments, expected) in cases {
            let arguments: Vec<_> = arguments.into_iter().map(str::to_string).collect();
            assert_eq!(expand_arguments(&arguments, file), expected);
        }
    }
}
