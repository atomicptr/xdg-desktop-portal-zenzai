use zbus::{fdo, interface};
use zvariant::Value;

use crate::portals::settings::config::{AccentColor, Contrast};

use super::config::{ColorScheme, SettingsConf};

pub struct SettingsService {
    pub config: SettingsConf,
}

#[interface(name = "org.freedesktop.portal.Settings")]
impl SettingsService {
    async fn read(&self, namespace: &str, key: &str) -> fdo::Result<Value<'_>> {
        println!("read: {}.{}", namespace, key);

        if namespace != "org.freedesktop.appearance" {
            return Err(fdo::Error::Failed(format!(
                "porta: unknown namespace {}",
                namespace
            )));
        }

        match key {
            "color-scheme" => match &self.config.color_scheme {
                None => Ok(Value::U32(ColorScheme::default().into())),
                Some(color_scheme) => Ok(Value::U32(color_scheme.clone().into())),
            },
            "contrast" => match &self.config.contrast {
                None => Ok(Value::U32(Contrast::default().into())),
                Some(contrast) => Ok(Value::U32(contrast.clone().into())),
            },
            "accent-color" => match &self.config.accent_color {
                None => Err(fdo::Error::Failed(format!(
                    "porta: no accent color defined"
                ))),
                Some(AccentColor { r, g, b }) => Ok((
                    Value::F64(r.clone().into()),
                    Value::F64(g.clone().into()),
                    Value::F64(b.clone().into()),
                )
                    .into()),
            },
            key => Err(fdo::Error::Failed(format!(
                "porta: unsupported key: {}.{}",
                namespace, key
            ))),
        }
    }
}
