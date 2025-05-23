use std::collections::HashMap;

use serde::Deserialize;

pub type SettingsMap = HashMap<String, HashMap<String, SettingsMapValue>>;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SettingsConfig {
    pub enabled: bool,
    pub color_scheme: Option<ColorScheme>,
    pub accent_color: Option<AccentColor>,
    pub contrast: Option<Contrast>,
    pub dict: Option<SettingsMap>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    #[default]
    NoPreference,
    Dark,
    Light,
}

impl Into<u32> for ColorScheme {
    fn into(self) -> u32 {
        match self {
            Self::NoPreference => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum AccentColor {
    ColorString(String),
    RGB(ColorRGB),
}

#[derive(Debug, Deserialize, Clone)]
pub struct ColorRGB {
    r: u8,
    g: u8,
    b: u8,
}

impl AccentColor {
    pub fn to_color_tuple(&self) -> Option<(f64, f64, f64)> {
        match self {
            AccentColor::ColorString(str) => {
                if let Ok(color) = csscolorparser::parse(str) {
                    let [r, g, b, _] = color.to_array();
                    Some((r.into(), g.into(), b.into()))
                } else {
                    None
                }
            }
            AccentColor::RGB(ColorRGB { r, g, b }) => Some((
                (r.clone() as f64) / 255.0,
                (g.clone() as f64) / 255.0,
                (b.clone() as f64) / 255.0,
            )),
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum Contrast {
    #[default]
    NoPreference,
    High,
}

impl Into<u32> for Contrast {
    fn into(self) -> u32 {
        match self {
            Self::NoPreference => 0,
            Self::High => 1,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum SettingsMapValue {
    String(String),
    Int(i64),
    Bool(bool),
    Float(f64),
}

impl From<String> for SettingsMapValue {
    fn from(value: String) -> Self {
        SettingsMapValue::String(value)
    }
}
impl From<i64> for SettingsMapValue {
    fn from(value: i64) -> Self {
        SettingsMapValue::Int(value)
    }
}

impl From<bool> for SettingsMapValue {
    fn from(value: bool) -> Self {
        SettingsMapValue::Bool(value)
    }
}

impl From<f64> for SettingsMapValue {
    fn from(value: f64) -> Self {
        SettingsMapValue::Float(value)
    }
}
