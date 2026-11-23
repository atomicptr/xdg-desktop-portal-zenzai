use anyhow::{Result, ensure};
use serde::Deserialize;
use zenzai_core::{
    appchooser::{AppChooserConfig, TerminalName},
    appearance::ThemeConfig,
};

use crate::portals::{
    access::AccessConfig, account::AccountConfig, dynamic_launcher::DynamicLauncherConfig,
    email::EmailConfig, notification::config::NotificationConfig, print::config::PrintConfig,
    settings::config::SettingsConfig, usb::UsbConfig,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub access: AccessConfig,
    pub account: AccountConfig,
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

impl Config {
    pub fn restrict_portals(&mut self, whitelist: Option<&str>) -> Result<()> {
        let Some(whitelist) = whitelist else {
            return Ok(());
        };
        let names: Vec<_> = whitelist
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        let portals = [
            ("access", &mut self.access.enabled),
            ("account", &mut self.account.enabled),
            ("usb", &mut self.usb.enabled),
            ("settings", &mut self.settings.enabled),
            ("appchooser", &mut self.appchooser.enabled),
            ("dynamic-launcher", &mut self.dynamic_launcher.enabled),
            ("email", &mut self.email.enabled),
            ("print", &mut self.print.enabled),
            ("notification", &mut self.notification.enabled),
        ];
        for name in &names {
            ensure!(
                portals.iter().any(|(known, _)| known == name),
                "unknown portal in whitelist: {name:?}"
            );
        }

        for (name, enabled) in portals {
            *enabled &= names.contains(&name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use zenzai_core::appearance::{ColorScheme, UiConfig};

    use crate::portals::settings::config::Contrast;

    use super::*;

    // Shared with the Nix module check to cover every generated option.
    const FULL_CONFIG: &str = include_str!("../tests/full-config.toml");

    fn flags(config: &Config) -> [bool; 9] {
        [
            config.access.enabled,
            config.account.enabled,
            config.usb.enabled,
            config.settings.enabled,
            config.appchooser.enabled,
            config.dynamic_launcher.enabled,
            config.email.enabled,
            config.print.enabled,
            config.notification.enabled,
        ]
    }

    #[test]
    fn portal_enablement_is_opt_out() {
        let sections = [
            "access",
            "account",
            "usb",
            "settings",
            "appchooser",
            "dynamic-launcher",
            "email",
            "print",
            "notification",
        ];

        assert!(flags(&Config::default()).into_iter().all(|enabled| enabled));

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
                flags(&config)
                    .into_iter()
                    .all(|enabled| enabled == value.unwrap_or(true))
            );
        }

        let config: Config = toml::from_str("[usb]\nenabled = false").unwrap();
        assert_eq!(
            flags(&config),
            [true, true, false, true, true, true, true, true, true]
        );
        assert!(
            flags(&toml::from_str("").unwrap())
                .into_iter()
                .all(|enabled| enabled)
        );
    }

    #[test]
    fn whitelist_only_restricts_configured_portals() {
        let mut config = Config::default();
        config.restrict_portals(None).unwrap();
        assert_eq!(flags(&config), [true; 9]);
        assert!(config.restrict_portals(Some("account,typo")).is_err());
        assert_eq!(flags(&config), [true; 9]);

        config
            .restrict_portals(Some(" account, appchooser,account, "))
            .unwrap();
        assert_eq!(
            flags(&config),
            [false, true, false, false, true, false, false, false, false]
        );
        config.account.enabled = false;
        config.restrict_portals(Some("account,appchooser")).unwrap();
        assert_eq!(
            flags(&config),
            [false, false, false, false, true, false, false, false, false]
        );

        for (index, name) in [
            "access",
            "account",
            "usb",
            "settings",
            "appchooser",
            "dynamic-launcher",
            "email",
            "print",
            "notification",
        ]
        .into_iter()
        .enumerate()
        {
            let mut config = Config::default();
            config.restrict_portals(Some(name)).unwrap();
            let mut expected = [false; 9];
            expected[index] = true;
            assert_eq!(flags(&config), expected, "{name}");
            config.restrict_portals(Some(name)).unwrap();
            assert_eq!(flags(&config), expected);
        }

        for empty in ["", " , "] {
            let mut config = Config::default();
            config.restrict_portals(Some(empty)).unwrap();
            assert_eq!(flags(&config), [false; 9]);
        }
    }

    #[test]
    fn parses_every_option_generated_by_the_nix_module() {
        let config: Config = toml::from_str(FULL_CONFIG).unwrap();

        assert_eq!(config.terminal.unwrap().as_str(), "wezterm");

        let access = config.access;
        assert!(access.enabled);
        assert_eq!(access.command.as_deref(), Some("zenzai-dialog-access"));

        let account = config.account;
        assert!(account.enabled);
        assert_eq!(account.command.as_deref(), Some("zenzai-dialog-access"));
        assert_eq!(account.id.as_deref(), Some("zenzai"));
        assert_eq!(account.name.as_deref(), Some("Zenzai User"));
        assert_eq!(
            account.image.unwrap().to_str(),
            Some("/home/zenzai/Pictures/avatar.png")
        );

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
