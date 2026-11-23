use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
    time::Duration,
};

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{
    sync::{mpsc, watch},
    time,
};
use zbus::Connection;
use zenzai_core::config::{config_paths, load_or_default};

use crate::{
    config::Config,
    constants::DBUS_PATH,
    portals::{
        access::DIALOG_COMMAND as ACCESS_DIALOG_COMMAND, appchooser::service::SELECT_COMMAND,
        dynamic_launcher::DIALOG_COMMAND as LAUNCHER_DIALOG_COMMAND, print::dialog::DIALOG_COMMAND,
        settings::service::SettingsService,
    },
    utils::command::resolve_command,
};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(200);
const MAX_SYMLINKS: usize = 40;
const WATCH_REFRESH_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryId {
    device: u64,
    inode: u64,
}

fn watch_directories(targets: &HashSet<PathBuf>) -> HashMap<PathBuf, DirectoryId> {
    targets
        .iter()
        .filter_map(|path| {
            let path = path.canonicalize().ok()?;
            let metadata = path.metadata().ok()?;
            metadata.is_dir().then_some((
                path,
                DirectoryId {
                    device: metadata.dev(),
                    inode: metadata.ino(),
                },
            ))
        })
        .collect()
}

// Portal enablement flags are independent, not mutually exclusive states.
#[allow(clippy::struct_excessive_bools)]
#[derive(PartialEq, Eq)]
pub struct EnabledPortals {
    access: bool,
    settings: bool,
    appchooser: bool,
    dynamic_launcher: bool,
    email: bool,
    print: bool,
    notification: bool,
}

impl From<&Config> for EnabledPortals {
    fn from(config: &Config) -> Self {
        Self {
            access: config.access.as_ref().is_some_and(|config| config.enabled),
            settings: config
                .settings
                .as_ref()
                .is_some_and(|config| config.enabled),
            appchooser: config
                .appchooser
                .as_ref()
                .is_some_and(|config| config.enabled),
            dynamic_launcher: config
                .dynamic_launcher
                .as_ref()
                .is_some_and(|config| config.enabled),
            email: config.email.as_ref().is_some_and(|config| config.enabled),
            print: config.print.as_ref().is_some_and(|config| config.enabled),
            notification: config
                .notification
                .as_ref()
                .is_some_and(|config| config.enabled),
        }
    }
}

pub struct ConfigWatcher {
    watcher: RecommendedWatcher,
    events: mpsc::Receiver<()>,
    directories: HashMap<PathBuf, DirectoryId>,
    rescan: mpsc::Sender<()>,
    targets: Arc<RwLock<HashSet<PathBuf>>>,
}

impl ConfigWatcher {
    pub fn new() -> Result<Self> {
        // Coalesce events without allowing an editor's event burst to grow a queue.
        let (sender, events) = mpsc::channel(1);
        let targets = Arc::new(RwLock::new(HashSet::new()));
        let event_targets = Arc::clone(&targets);
        let rescan = sender.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            match event {
                Ok(event) => {
                    if event.kind.is_access() {
                        return;
                    }

                    let targets = event_targets.read().unwrap_or_else(PoisonError::into_inner);
                    if !event.paths.is_empty()
                        && !event.paths.iter().any(|path| targets.contains(path))
                    {
                        return;
                    }
                }
                Err(err) => tracing::warn!("config watcher: {err}"),
            }

            let _ = sender.try_send(());
        })?;
        let mut watcher = Self {
            watcher,
            events,
            directories: HashMap::new(),
            rescan,
            targets,
        };
        watcher.refresh();
        Ok(watcher)
    }

    fn refresh(&mut self) {
        for _ in 0..WATCH_REFRESH_ATTEMPTS {
            let targets = watch_targets();
            let directories = watch_directories(&targets);
            self.targets
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(targets.iter().cloned());

            // notify can drop a watch after a directory moves away and back.
            // Reassert unchanged watches without interrupting their coverage.
            for (directory, identity) in &directories {
                if self
                    .directories
                    .get(directory)
                    .is_some_and(|previous| previous != identity)
                {
                    let _ = self.watcher.unwatch(directory);
                }

                match self.watcher.watch(directory, RecursiveMode::NonRecursive) {
                    Ok(()) => {
                        self.directories.insert(directory.clone(), *identity);
                    }
                    Err(err) => tracing::warn!("could not watch {}: {err}", directory.display()),
                }
            }

            self.directories.retain(|directory, _| {
                if directories.contains_key(directory) {
                    return true;
                }

                let _ = self.watcher.unwatch(directory);
                false
            });

            // A replacement during discovery/arming must not leave us watching
            // the old generation indefinitely. Scan again with watches armed.
            let current_targets = watch_targets();
            if targets == current_targets && directories == watch_directories(&current_targets) {
                *self.targets.write().unwrap_or_else(PoisonError::into_inner) = targets;
                return;
            }
        }

        // Yield under continuous topology changes rather than busy-looping.
        let _ = self.rescan.try_send(());
    }

    pub async fn changed(&mut self) {
        // The rescan sender keeps this channel open for the watcher's lifetime.
        self.events.recv().await;

        time::sleep(SAVE_DEBOUNCE).await;
        while self.events.try_recv().is_ok() {}
        self.refresh();
    }
}

// Include the lexical paths, intermediate symlinks and their targets. In
// particular, /etc/xdg can point through /etc/static into the Nix store.
fn watch_targets() -> HashSet<PathBuf> {
    let mut targets = HashSet::new();
    let mut seen = HashSet::new();
    let mut pending: Vec<_> = config_paths().into_iter().map(|path| (path, 0)).collect();
    while let Some((path, depth)) = pending.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }

        for ancestor in path.ancestors() {
            targets.insert(ancestor.to_path_buf());
            let suffix = path.strip_prefix(ancestor).unwrap_or(Path::new(""));
            // Keep missing descendants when resolving a symlinked parent.
            if let Ok(resolved) = ancestor.canonicalize() {
                pending.push((resolved.join(suffix), depth));
            }

            if depth < MAX_SYMLINKS
                && let Ok(target) = fs::read_link(ancestor)
                && let Some(parent) = ancestor.parent()
            {
                pending.push((parent.join(target).join(suffix), depth + 1));
            }
        }
    }

    targets
}

pub struct Reload {
    pub enabled: EnabledPortals,
    pub access: Option<watch::Sender<PathBuf>>,
    pub appchooser: Option<watch::Sender<PathBuf>>,
    pub dynamic_launcher: Option<watch::Sender<PathBuf>>,
    pub email: Option<watch::Sender<PathBuf>>,
    pub print: Option<watch::Sender<PathBuf>>,
}

impl Reload {
    pub async fn apply(&mut self, connection: &Connection) -> Result<()> {
        // Deserialize the entire file before mutating any running service.
        let config: Config = load_or_default()?;
        let enabled = EnabledPortals::from(&config);
        if enabled != self.enabled {
            tracing::warn!(
                "portal enable/disable changes require a restart; keeping current interfaces"
            );
            self.enabled = enabled;
        }

        if let Ok(interface) = connection
            .object_server()
            .interface::<_, SettingsService>(DBUS_PATH)
            .await
        {
            let changes = interface
                .get_mut()
                .await
                .update(config.settings.unwrap_or_default());
            for change in changes {
                if let Err(err) = SettingsService::setting_changed(
                    interface.signal_emitter(),
                    change.namespace,
                    change.key,
                    change.value,
                )
                .await
                {
                    tracing::warn!("could not emit SettingChanged: {err}");
                }
            }
        }

        if let Some(sender) = &self.access {
            let config = config.access.unwrap_or_default();
            update_command(
                sender,
                config.command.as_deref().unwrap_or(ACCESS_DIALOG_COMMAND),
            );
        }
        if let Some(sender) = &self.appchooser {
            let config = config.appchooser.unwrap_or_default();
            update_command(sender, config.command.as_deref().unwrap_or(SELECT_COMMAND));
        }

        if let Some(sender) = &self.dynamic_launcher {
            let config = config.dynamic_launcher.unwrap_or_default();
            update_command(
                sender,
                config.command.as_deref().unwrap_or(LAUNCHER_DIALOG_COMMAND),
            );
        }

        if let Some(sender) = &self.email {
            let config = config.email.unwrap_or_default();
            update_command(sender, config.command.as_deref().unwrap_or(SELECT_COMMAND));
        }

        if let Some(sender) = &self.print {
            let config = config.print.unwrap_or_default();
            update_command(sender, config.command.as_deref().unwrap_or(DIALOG_COMMAND));
        }

        Ok(())
    }
}

fn update_command(sender: &watch::Sender<PathBuf>, command: &str) {
    match resolve_command(command) {
        Ok(command) => {
            sender.send_replace(command);
        }
        Err(err) => {
            tracing::warn!("keeping previous portal command, could not find {command:?}: {err}");
        }
    }
}
