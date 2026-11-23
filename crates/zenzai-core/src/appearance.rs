use csscolorparser::Color;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    #[default]
    NoPreference,
    Dark,
    Light,
}

impl From<ColorScheme> for u32 {
    fn from(value: ColorScheme) -> Self {
        match value {
            ColorScheme::NoPreference => 0,
            ColorScheme::Dark => 1,
            ColorScheme::Light => 2,
        }
    }
}

// Keep the pre-0.4.0 RGB config format.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum AccentColor {
    Css(Color),
    Rgb { r: u8, g: u8, b: u8 },
}

impl AccentColor {
    #[must_use]
    pub fn color(&self) -> Color {
        match self {
            Self::Css(color) => *color,
            Self::Rgb { r, g, b } => Color::from_rgba8(*r, *g, *b, u8::MAX),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct UiConfig {
    #[serde(rename = "icon-theme")]
    pub icon_theme: Option<String>,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub settings: AppearanceConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppearanceConfig {
    pub color_scheme: Option<ColorScheme>,
    pub accent_color: Option<AccentColor>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ThemeConfig {
    pub name: Option<String>,
    pub font: Option<String>,
    pub text_size: Option<f32>,
    #[serde(default)]
    pub palette: PaletteConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct PaletteConfig {
    pub background: Option<Color>,
    pub text: Option<Color>,
    pub primary: Option<Color>,
    pub success: Option<Color>,
    pub warning: Option<Color>,
    pub danger: Option<Color>,
}
