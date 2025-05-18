use serde::{Deserialize, Deserializer};

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

// TODO: allow specifying accent color as hex or 0-255 segments
#[derive(Debug, Deserialize, Default, Clone)]
pub struct AccentColor {
    #[serde(deserialize_with = "deserialize_rgb")]
    pub r: f32,
    #[serde(deserialize_with = "deserialize_rgb")]
    pub g: f32,
    #[serde(deserialize_with = "deserialize_rgb")]
    pub b: f32,
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

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SettingsConf {
    pub enabled: bool,
    pub color_scheme: Option<ColorScheme>,
    pub accent_color: Option<AccentColor>,
    pub contrast: Option<Contrast>,
}

fn deserialize_rgb<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;

    if value >= 0.0 && value <= 1.0 {
        return Ok(value);
    }

    Err(serde::de::Error::custom(format!(
        "RGB component {} is out of range 0.0..1.0",
        value
    )))
}
