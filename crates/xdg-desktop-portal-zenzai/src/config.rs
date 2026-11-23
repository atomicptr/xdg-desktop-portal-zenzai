use serde::Deserialize;
use zenzai_core::{
    appchooser::{AppChooserConfig, TerminalName},
    appearance::ThemeConfig,
};

use crate::portals::{
    access::AccessConfig, dynamic_launcher::DynamicLauncherConfig, email::EmailConfig,
    notification::config::NotificationConfig, print::config::PrintConfig,
    settings::config::SettingsConfig, usb::UsbConfig,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub access: AccessConfig,
    pub usb: UsbConfig,
    pub terminal: Option<TerminalName>,
    pub settings: SettingsConfig,
    pub appchooser: AppChooserConfig,
    #[serde(rename = "dynamic-launcher")]
    pub dynamic_launcher: DynamicLauncherConfig,
    pub email: EmailConfig,
    pub print: PrintConfig,
    pub notification: NotificationConfig,
    #[serde(rename = "icon-theme")]
    _icon_theme: Option<String>,
    // Validate the shared GUI schema even though the daemon does not use it.
    #[serde(rename = "theme")]
    _theme: Option<ThemeConfig>,
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use zenzai_core::appearance::{ColorScheme, UiConfig};

    use crate::portals::settings::config::Contrast;

    use super::*;

    // Shared with the Nix module check to cover every generated option.
    const FULL_CONFIG: &str = include_str!("../tests/full-config.toml");

    #[test]
    fn portal_enablement_is_opt_out() {
        let sections = [
            "access",
            "usb",
            "settings",
            "appchooser",
            "dynamic-launcher",
            "email",
            "print",
            "notification",
        ];
        let flags = |config: Config| {
            [
                config.access.enabled,
                config.usb.enabled,
                config.settings.enabled,
                config.appchooser.enabled,
                config.dynamic_launcher.enabled,
                config.email.enabled,
                config.print.enabled,
                config.notification.enabled,
            ]
        };

        assert!(flags(Config::default()).into_iter().all(|enabled| enabled));

        for value in [None, Some(true), Some(false)] {
            let input = sections.iter().fold(String::new(), |mut input, section| {
                writeln!(input, "[{section}]").unwrap();
                if let Some(enabled) = value {
                    writeln!(input, "enabled = {enabled}").unwrap();
                }

                input
            });
            let config: Config = toml::from_str(&input).unwrap();
            assert!(
                flags(config)
                    .into_iter()
                    .all(|enabled| enabled == value.unwrap_or(true))
            );
        }

        let config: Config = toml::from_str("[usb]\nenabled = false").unwrap();
        assert_eq!(
            flags(config),
            [true, false, true, true, true, true, true, true]
        );
        assert!(
            flags(toml::from_str("").unwrap())
                .into_iter()
                .all(|enabled| enabled)
        );
    }

    #[test]
    fn parses_every_option_generated_by_the_nix_module() {
        let config: Config = toml::from_str(FULL_CONFIG).unwrap();

        assert_eq!(config.terminal.unwrap().as_str(), "wezterm");

        let access = config.access;
        assert!(access.enabled);
        assert_eq!(access.command.as_deref(), Some("zenzai-dialog-access"));

        let usb = config.usb;
        assert!(usb.enabled);
        assert_eq!(usb.command.as_deref(), Some("zenzai-dialog-access"));

        let settings = config.settings;
        assert!(settings.enabled);
        assert_eq!(settings.color_scheme, ColorScheme::Dark);
        assert!(settings.accent_color.is_some());
        assert!(matches!(settings.contrast, Contrast::High));
        assert_eq!(settings.dict["org.gnome.SomeApp"].len(), 2);

        let appchooser = config.appchooser;
        assert!(appchooser.enabled);
        assert!(appchooser.command.is_some());
        assert!(!appchooser.write_mimeapps);
        assert_eq!(appchooser.defaults.len(), 3);

        let dynamic_launcher = config.dynamic_launcher;
        assert!(dynamic_launcher.enabled);
        assert_eq!(
            dynamic_launcher.command.as_deref(),
            Some("zenzai-dialog-launcher")
        );

        let email = config.email;
        assert!(email.enabled);
        assert_eq!(email.command.as_deref(), Some("zenzai-app-select"));

        let print = config.print;
        assert!(print.enabled);
        assert!(print.command.is_some());

        assert!(config.notification.enabled);

        let ui: UiConfig = toml::from_str(FULL_CONFIG).unwrap();
        assert_eq!(ui.icon_theme.as_deref(), Some("Papirus"));
        let palette = &ui.theme.palette;
        assert!(ui.theme.name.is_some() && ui.theme.font.is_some() && ui.theme.text_size.is_some());
        assert!(
            [
                &palette.background,
                &palette.text,
                &palette.primary,
                &palette.success,
                &palette.warning,
                &palette.danger,
            ]
            .iter()
            .all(|color| color.is_some())
        );

        let invalid_theme = FULL_CONFIG.replace("text-size = 14", "text-size = \"invalid\"");
        assert_ne!(invalid_theme, FULL_CONFIG);
        assert!(toml::from_str::<Config>(&invalid_theme).is_err());
    }
}
