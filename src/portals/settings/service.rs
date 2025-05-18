use std::collections::HashMap;

use zbus::{fdo, interface};
use zvariant::Value;

use crate::portals::settings::config::{AccentColor, Contrast};

use super::config::{ColorScheme, SettingsConf};

const NAMESPACE: &'static str = "org.freedesktop.appearance";

pub struct SettingsService {
    pub config: SettingsConf,
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsService {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        0
    }

    // read is deprecated, not yet sure how .ReadOne is different so we just call it as-is
    async fn read(&self, namespace: &str, key: &str) -> fdo::Result<Value<'_>> {
        self.read_one(namespace, key).await
    }

    async fn read_one(&self, namespace: &str, key: &str) -> fdo::Result<Value<'_>> {
        tracing::info!("read_one: {}.{}", namespace, key);

        if namespace != NAMESPACE {
            return Err(fdo::Error::Failed(format!(
                "zenzai: unknown namespace {}",
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
                    "zenzai: no accent color defined"
                ))),
                Some(AccentColor { r, g, b }) => Ok((
                    Value::F64(r.clone().into()),
                    Value::F64(g.clone().into()),
                    Value::F64(b.clone().into()),
                )
                    .into()),
            },
            key => Err(fdo::Error::Failed(format!(
                "zenzai: unsupported key: {}.{}",
                namespace, key
            ))),
        }
    }

    async fn read_all(&self, namespaces: Vec<&str>) -> fdo::Result<Value<'_>> {
        tracing::info!("read_all: {:?}", namespaces);

        let mut result = HashMap::new();

        for ns in namespaces {
            if ns != NAMESPACE {
                continue;
            }

            let mut nsmap = HashMap::new();

            let color_scheme = self
                .config
                .color_scheme
                .clone()
                .unwrap_or(ColorScheme::default());
            nsmap.insert("color-scheme", Value::U32(color_scheme.clone().into()));

            let contrast = self.config.contrast.clone().unwrap_or(Contrast::default());
            nsmap.insert("contrast", Value::U32(contrast.clone().into()));

            if let Some(AccentColor { r, g, b }) = &self.config.accent_color {
                nsmap.insert(
                    "accent-color",
                    (
                        Value::F64(r.clone().into()),
                        Value::F64(g.clone().into()),
                        Value::F64(b.clone().into()),
                    )
                        .into(),
                );
            }

            result.insert(NAMESPACE, nsmap);
        }

        Ok(result.into())
    }
}
