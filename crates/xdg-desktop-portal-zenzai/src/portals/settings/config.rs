use std::collections::HashMap;

use serde::Deserialize;
use zbus::Variant;
use zenzai_core::appearance::{AccentColor, ColorScheme};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SettingsConfig {
    pub enabled: bool,
    #[serde(default)]
    pub color_scheme: ColorScheme,
    pub accent_color: Option<AccentColor>,
    #[serde(default)]
    pub contrast: Contrast,
    #[serde(default)]
    pub dict: HashMap<String, HashMap<String, SettingsMapValue>>,
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Contrast {
    #[default]
    NoPreference,
    High,
}

impl From<Contrast> for u32 {
    fn from(val: Contrast) -> Self {
        match val {
            Contrast::NoPreference => 0,
            Contrast::High => 1,
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

impl From<&SettingsMapValue> for Variant<'static> {
    fn from(value: &SettingsMapValue) -> Self {
        match value {
            SettingsMapValue::String(value) => Variant::from(value.clone()),
            SettingsMapValue::Int(value) => Variant::from(*value),
            SettingsMapValue::Bool(value) => Variant::from(*value),
            SettingsMapValue::Float(value) => Variant::from(*value),
        }
    }
}
