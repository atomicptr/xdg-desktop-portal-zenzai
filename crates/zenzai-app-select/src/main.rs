use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;
use serde::Deserialize;
use url::Url;
use zenzai_core::{
    appchooser::{AppChooserConfig, DefaultMapping, Target, TerminalName},
    config::{load_or_default, load_state},
    ipc::{AppSelectRequest, EXIT_CANCELLED, EXIT_ERROR, EXIT_SUCCESS},
};

use apps::App;

mod apps;
mod email;
mod mimeapps;
mod ui;

const STATE_FILE: &str = "app-select.toml";
const APP_DIRECTORY: &str = "xdg-desktop-portal-zenzai";

#[derive(Parser)]
#[command(
    version,
    about = "Open a file with a configured or interactively selected application"
)]
struct Cli {
    /// File to open
    file: Option<PathBuf>,
    /// Always show the chooser, even when a default application is available
    #[arg(long)]
    force: bool,
    /// Read a portal request from stdin
    #[arg(long, hide = true, conflicts_with = "file")]
    portal: bool,
}

#[derive(Default, Deserialize)]
struct Config {
    terminal: Option<TerminalName>,
    #[serde(default)]
    appchooser: AppChooserConfig,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match run(Cli::parse()) {
        Ok(true) => ExitCode::from(EXIT_SUCCESS as u8),
        Ok(false) => ExitCode::from(EXIT_CANCELLED as u8),
        Err(err) => {
            eprintln!("zenzai-app-select: {err:#}");
            ExitCode::from(EXIT_ERROR as u8)
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
    let request = if cli.portal {
        serde_json::from_reader(io::stdin().lock())
            .context("could not parse app selection request")?
    } else {
        let path = cli
            .file
            .context("no file given, see --help")?
            .canonicalize()
            .context("could not resolve file")?;
        AppSelectRequest {
            content_type: content_type(&path)?,
            uri: Url::from_file_path(path)
                .map_err(|()| anyhow!("invalid file path"))?
                .to_string(),
            ..Default::default()
        }
    };

    let config: Config = load_or_default()?;
    let terminal = apps::terminal(config.terminal.as_ref().map(TerminalName::as_str));
    let local = Url::parse(&request.uri)
        .ok()
        .and_then(|uri| uri.to_file_path().ok());
    let file = local.as_ref().map_or_else(
        || request.uri.clone(),
        |path| path.to_string_lossy().into_owned(),
    );
    let mapping = config.appchooser.mapping(&request.content_type);
    if !cli.force
        && let Some(DefaultMapping::One(target @ Target::Command(_))) = mapping
    {
        let mut apps = Vec::new();
        let index = apps::resolve(target, &mut apps, &file)?;
        apps[index].launch(&terminal, &request)?;
        return Ok(true);
    }

    let mut apps = apps::discover(&request.uri);

    if !cli.force {
        if let Some(DefaultMapping::One(target)) = mapping {
            match apps::resolve(target, &mut apps, &file) {
                Ok(index) => {
                    apps[index].launch(&terminal, &request)?;
                    return Ok(true);
                }
                Err(err) => tracing::warn!("configured default is unavailable: {err:#}"),
            }
        } else if mapping.is_none() {
            let remembered: BTreeMap<String, String> = load_state(STATE_FILE);
            let index = remembered
                .get(&request.content_type)
                .and_then(|id| apps.iter().position(|app| app.matches_id(id)))
                .or_else(|| match mimeapps::default_app(&request.content_type) {
                    Ok(Some(id)) => {
                        let index = apps.iter().position(|app| app.matches_id(&id));
                        if index.is_none() {
                            tracing::warn!("MIME default is unavailable: {id}");
                        }
                        index
                    }
                    Ok(None) => None,
                    Err(err) => {
                        tracing::warn!("could not look up MIME default: {err:#}");
                        None
                    }
                });
            if let Some(index) = index {
                apps[index].launch(&terminal, &request)?;
                return Ok(true);
            }
        }
    }

    let suggested = if let Some(DefaultMapping::Choice(targets)) = mapping {
        let mut indices = Vec::new();
        for target in targets {
            match apps::resolve(target, &mut apps, &file) {
                Ok(index) => {
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
                Err(err) => tracing::warn!("skipping configured choice: {err:#}"),
            }
        }
        indices
    } else {
        let compatible = apps::compatible_types(&request.content_type);
        let mut indices: Vec<_> = apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                app.menu_visible
                    && app
                        .content_types
                        .iter()
                        .any(|mime| compatible.contains(mime))
            })
            .map(|(index, _)| index)
            .collect();
        indices.sort_by_key(|index| !request.choices.iter().any(|id| apps[*index].matches_id(id)));
        indices
    };

    let title = if request.email.is_some() {
        "Choose an email application".to_string()
    } else {
        local
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || request.uri.clone(),
                |name| name.to_string_lossy().into_owned(),
            )
    };
    let apps: Arc<[App]> = apps.into();
    let Some((index, remember)) = ui::run(apps.clone(), suggested, title, mapping.is_some())?
    else {
        return Ok(false);
    };

    let app = &apps[index];
    app.launch(&terminal, &request)?;

    if remember && let Some(id) = &app.id {
        if let Err(err) = remember_app(&request.content_type, id) {
            tracing::warn!("application opened, but could not save its local association: {err:#}");
        }

        if config.appchooser.write_mimeapps
            && let Err(err) = mimeapps::remember(&request.content_type, id)
        {
            tracing::warn!("application opened, but could not update MIME associations: {err:#}");
        }
    }

    Ok(true)
}

fn content_type(path: &Path) -> Result<String> {
    let output = Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .output()
        .context("could not detect MIME type (xdg-mime is required)")?;
    ensure!(
        output.status.success(),
        "MIME detection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mime = String::from_utf8(output.stdout)?.trim().to_string();
    ensure!(
        mime.contains('/') && !mime.contains(char::is_whitespace),
        "invalid MIME type: {mime:?}"
    );
    Ok(mime)
}

fn remember_app(content_type: &str, id: &str) -> Result<()> {
    let directory = dirs::state_dir()
        .context("could not determine state directory")?
        .join(APP_DIRECTORY);
    fs::create_dir_all(&directory)?;
    // Lock a separate inode: replacing the state file must not invalidate the lock.
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("app-select.lock"))?;
    lock.lock()?;

    let mut associations: BTreeMap<String, String> = load_state(STATE_FILE);
    associations.insert(content_type.to_string(), id.to_string());
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(toml::to_string(&associations)?.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(directory.join(STATE_FILE))?;
    Ok(())
}
