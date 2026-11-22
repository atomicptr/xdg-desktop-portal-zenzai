use serde::Deserialize;
use zenzai_core::appchooser::{AppChooserConfig, TerminalName};

use crate::portals::{print::config::PrintConfig, settings::config::SettingsConfig};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub terminal: Option<TerminalName>,
    pub settings: Option<SettingsConfig>,
    pub appchooser: Option<AppChooserConfig>,
    pub print: Option<PrintConfig>,
}

#[cfg(test)]
mod tests {
    use zenzai_core::appearance::{ColorScheme, UiConfig};

    use crate::portals::settings::config::Contrast;

    use super::*;

    // shared with the Nix module check, every option the module can set must be understood here
    const FULL_CONFIG: &str = include_str!("../tests/full-config.toml");

    #[test]
    fn parses_every_option_generated_by_the_nix_module() {
        let config: Config = toml::from_str(FULL_CONFIG).unwrap();

        assert_eq!(config.terminal.unwrap().as_str(), "wezterm");

        let settings = config.settings.unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.color_scheme, ColorScheme::Dark);
        assert!(settings.accent_color.is_some());
        assert!(matches!(settings.contrast, Contrast::High));
        assert_eq!(settings.dict["org.gnome.SomeApp"].len(), 2);

        let appchooser = config.appchooser.unwrap();
        assert!(appchooser.enabled);
        assert!(appchooser.command.is_some());
        assert_eq!(appchooser.defaults.len(), 3);

        let print = config.print.unwrap();
        assert!(print.enabled);
        assert!(print.command.is_some());

        let ui: UiConfig = toml::from_str(FULL_CONFIG).unwrap();
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
    }
}
