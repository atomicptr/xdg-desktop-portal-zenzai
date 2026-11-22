use iced::{Color, Theme, theme::Palette};

use crate::appearance::{ColorScheme, PaletteConfig, UiConfig};

const CUSTOM_THEME_NAME: &str = "zenzai";

pub fn theme(config: &UiConfig) -> Theme {
    let base = config
        .theme
        .name
        .as_deref()
        .and_then(builtin_theme)
        .unwrap_or(match config.settings.color_scheme {
            Some(ColorScheme::Dark) => Theme::Dark,
            _ => Theme::Light,
        });

    let accent = config
        .settings
        .accent_color
        .as_ref()
        .map(|c| iced_color(&c.color()));
    let palette = apply_overrides(base.palette(), &config.theme.palette, accent);

    if palette == base.palette() {
        return base;
    }

    Theme::custom(CUSTOM_THEME_NAME, palette)
}

fn iced_color(color: &csscolorparser::Color) -> Color {
    Color::from_rgba(color.r, color.g, color.b, color.a)
}

fn builtin_theme(name: &str) -> Option<Theme> {
    let wanted = name.trim().to_lowercase().replace([' ', '_'], "-");

    Theme::ALL
        .iter()
        .find(|theme| theme.to_string().to_lowercase().replace(' ', "-") == wanted)
        .cloned()
}

fn apply_overrides(base: Palette, overrides: &PaletteConfig, accent: Option<Color>) -> Palette {
    let color = |value: &Option<csscolorparser::Color>, fallback: Color| {
        value.as_ref().map_or(fallback, iced_color)
    };

    Palette {
        background: color(&overrides.background, base.background),
        text: color(&overrides.text, base.text),
        primary: color(&overrides.primary, accent.unwrap_or(base.primary)),
        success: color(&overrides.success, base.success),
        warning: color(&overrides.warning, base.warning),
        danger: color(&overrides.danger, base.danger),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_theme_names_are_matched_loosely() {
        assert_eq!(
            builtin_theme("catppuccin-mocha"),
            Some(Theme::CatppuccinMocha)
        );
        assert_eq!(
            builtin_theme("Tokyo Night Storm"),
            Some(Theme::TokyoNightStorm)
        );
        assert_eq!(builtin_theme("does-not-exist"), None);
    }
}
