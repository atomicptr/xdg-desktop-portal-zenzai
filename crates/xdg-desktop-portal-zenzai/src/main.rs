use anyhow::Result;
use config::Config;
use constants::{APP_VERSION, DBUS_NAME, DBUS_PATH};
use portals::{
    access::{AccessService, DIALOG_COMMAND as ACCESS_DIALOG_COMMAND},
    appchooser::service::{AppChooserService, SELECT_COMMAND},
    dynamic_launcher::{DIALOG_COMMAND as LAUNCHER_DIALOG_COMMAND, DynamicLauncherService},
    email::EmailService,
    notification::service::NotificationService,
    print::{dialog::DIALOG_COMMAND, service::PrintService},
    settings::service::SettingsService,
    usb::{DIALOG_COMMAND as USB_DIALOG_COMMAND, UsbService},
};
use reload::{ConfigWatcher, EnabledPortals, Reload};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;
use utils::command::resolve_command;
use zbus::conn::Builder;
use zenzai_core::config::load_or_default;

mod config;
mod constants;
mod portals;
mod reload;
mod utils;
rust_i18n::i18n!("../../locales", fallback = "en");

#[tokio::main]
async fn main() -> Result<()> {
    zenzai_core::i18n::init();

    // Tracing targets use the underscored crate name.
    let level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{}={level}", env!("CARGO_CRATE_NAME"))));

    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("xdg-desktop-portal-zenzai v{} started!", APP_VERSION);

    let mut watcher = ConfigWatcher::new()?;
    let config: Config = load_or_default()?;
    let mut reload = Reload {
        enabled: EnabledPortals::from(&config),
        access: None,
        usb: None,
        appchooser: None,
        dynamic_launcher: None,
        email: None,
        print: None,
    };

    tracing::debug!("Config: {:?}", config);

    drop(config.terminal);
    let mut any_enabled = false;

    let mut conn = Builder::session().name(DBUS_NAME);

    if let Some(config) = config.settings
        && config.enabled
    {
        any_enabled = true;

        tracing::info!("portal: org.freedesktop.portal.Settings enabled!");
        conn = conn.serve_at(DBUS_PATH, SettingsService::new(config));
    }

    if let Some(config) = config.access
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(ACCESS_DIALOG_COMMAND);
        match resolve_command(command) {
            Ok(command) => {
                any_enabled = true;
                tracing::info!("portal: org.freedesktop.portal.Access enabled!");
                let (sender, command) = watch::channel(command);
                reload.access = Some(sender);
                conn = conn.serve_at(DBUS_PATH, AccessService { command });
            }
            Err(err) => tracing::error!("Access disabled, could not find {command:?}: {err}"),
        }
    }

    if let Some(config) = config.usb
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(USB_DIALOG_COMMAND);

        match resolve_command(command) {
            Ok(command) => {
                any_enabled = true;

                tracing::info!("portal: org.freedesktop.portal.Usb enabled!");
                let (sender, command) = watch::channel(command);
                reload.usb = Some(sender);
                conn = conn.serve_at(DBUS_PATH, UsbService { command });
            }
            Err(err) => tracing::error!("Usb disabled, could not find {command:?}: {err}"),
        }
    }

    if let Some(config) = config.appchooser
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(SELECT_COMMAND);
        match resolve_command(command) {
            Ok(command) => {
                any_enabled = true;
                tracing::info!("portal: org.freedesktop.portal.AppChooser enabled!");
                let (sender, command) = watch::channel(command);
                reload.appchooser = Some(sender);
                conn = conn.serve_at(DBUS_PATH, AppChooserService { command });
            }
            Err(err) => tracing::error!("AppChooser disabled, could not find {command:?}: {err}"),
        }
    }

    if let Some(config) = config.dynamic_launcher
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(LAUNCHER_DIALOG_COMMAND);

        match resolve_command(command) {
            Ok(command) => {
                any_enabled = true;

                tracing::info!("portal: org.freedesktop.portal.DynamicLauncher enabled!");
                let (sender, command) = watch::channel(command);
                reload.dynamic_launcher = Some(sender);
                conn = conn.serve_at(DBUS_PATH, DynamicLauncherService { command });
            }
            Err(err) => {
                tracing::error!("DynamicLauncher disabled, could not find {command:?}: {err}");
            }
        }
    }

    if let Some(config) = config.email
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(SELECT_COMMAND);

        match resolve_command(command) {
            Ok(command) => {
                any_enabled = true;

                tracing::info!("portal: org.freedesktop.portal.Email enabled!");
                let (sender, command) = watch::channel(command);
                reload.email = Some(sender);
                conn = conn.serve_at(
                    DBUS_PATH,
                    EmailService {
                        chooser: AppChooserService { command },
                    },
                );
            }
            Err(err) => tracing::error!("Email disabled, could not find {command:?}: {err}"),
        }
    }

    if let Some(config) = config.print
        && config.enabled
    {
        let command = config.command.as_deref().unwrap_or(DIALOG_COMMAND);

        if let Ok(command) = resolve_command(command) {
            any_enabled = true;

            tracing::info!("portal: org.freedesktop.portal.Print enabled!");
            let (sender, command) = watch::channel(command);
            reload.print = Some(sender);
            conn = conn.serve_at(DBUS_PATH, PrintService::new(command));
        } else {
            tracing::error!(
                "portal: org.freedesktop.portal.Print disabled, could not find print dialog {command:?}"
            );
        }
    }

    if config.notification.is_some_and(|config| config.enabled) {
        any_enabled = true;
        tracing::info!("portal: org.freedesktop.portal.Notification enabled!");
        conn = conn.serve_at(DBUS_PATH, NotificationService::default());
    }

    if !any_enabled {
        tracing::error!("No portal was enabled, quitting");
        return Ok(());
    }

    let conn = conn.build().await?;
    // Reload once after registration to catch edits made during startup.
    loop {
        if let Err(err) = reload.apply(&conn).await {
            tracing::warn!("config reload rejected, keeping current configuration: {err:#}");
        }

        watcher.changed().await;
    }
}
