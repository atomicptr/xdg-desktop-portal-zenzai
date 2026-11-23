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
pub struct Config {
    pub access: Option<AccessConfig>,
    pub usb: Option<UsbConfig>,
    pub terminal: Option<TerminalName>,
    pub settings: Option<SettingsConfig>,
    pub appchooser: Option<AppChooserConfig>,
    #[serde(rename = "dynamic-launcher")]
    pub dynamic_launcher: Option<DynamicLauncherConfig>,
    pub email: Option<EmailConfig>,
    pub print: Option<PrintConfig>,
    pub notification: Option<NotificationConfig>,
    #[serde(rename = "icon-theme")]
    _icon_theme: Option<String>,
    // Validate the shared GUI schema even though the daemon does not use it.
    #[serde(rename = "theme")]
    _theme: Option<ThemeConfig>,
}

#[cfg(test)]
mod tests {
    use zenzai_core::appearance::{ColorScheme, UiConfig};

    use crate::portals::settings::config::Contrast;

    use super::*;

    // Shared with the Nix module check to cover every generated option.
    const FULL_CONFIG: &str = include_str!("../tests/full-config.toml");

    #[test]
    fn parses_every_option_generated_by_the_nix_module() {
        let config: Config = toml::from_str(FULL_CONFIG).unwrap();

        assert_eq!(config.terminal.unwrap().as_str(), "wezterm");

        let access = config.access.unwrap();
        assert!(access.enabled);
        assert_eq!(access.command.as_deref(), Some("zenzai-dialog-access"));

        let usb = config.usb.unwrap();
        assert!(usb.enabled);
        assert_eq!(usb.command.as_deref(), Some("zenzai-dialog-access"));

        let settings = config.settings.unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.color_scheme, ColorScheme::Dark);
        assert!(settings.accent_color.is_some());
        assert!(matches!(settings.contrast, Contrast::High));
        assert_eq!(settings.dict["org.gnome.SomeApp"].len(), 2);

        let appchooser = config.appchooser.unwrap();
        assert!(appchooser.enabled);
        assert!(appchooser.command.is_some());
        assert!(!appchooser.write_mimeapps);
        assert_eq!(appchooser.defaults.len(), 3);

        let dynamic_launcher = config.dynamic_launcher.unwrap();
        assert!(dynamic_launcher.enabled);
        assert_eq!(
            dynamic_launcher.command.as_deref(),
            Some("zenzai-dialog-launcher")
        );

        let email = config.email.unwrap();
        assert!(email.enabled);
        assert_eq!(email.command.as_deref(), Some("zenzai-app-select"));

        let print = config.print.unwrap();
        assert!(print.enabled);
        assert!(print.command.is_some());

        assert!(config.notification.unwrap().enabled);

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
