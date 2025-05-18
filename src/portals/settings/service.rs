use std::collections::HashMap;

use zbus::{fdo, interface, object_server::SignalEmitter};
use zvariant::Value;

use crate::portals::settings::config::Contrast;

use super::config::{ColorScheme, SettingsConfig};

const NAMESPACE: &'static str = "org.freedesktop.appearance";
const KEY_COLOR_SCHEME: &'static str = "color-scheme";
const KEY_CONTRAST: &'static str = "contrast";
const KEY_ACCENT_COLOR: &'static str = "accent-color";

pub struct SettingsService {
    pub config: SettingsConfig,
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsService {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        1
    }

    async fn read(&self, namespace: &str, key: &str) -> fdo::Result<Value<'_>> {
        tracing::debug!("request) read: {}.{}", namespace, key);

        if namespace != NAMESPACE {
            return Err(fdo::Error::Failed(format!(
                "zenzai: unknown namespace {}",
                namespace
            )));
        }

        let value = match key {
            KEY_COLOR_SCHEME => Ok(Value::U32(
                self.config.color_scheme.clone().unwrap_or_default().into(),
            )),
            KEY_CONTRAST => Ok(Value::U32(
                self.config.contrast.clone().unwrap_or_default().into(),
            )),
            KEY_ACCENT_COLOR => self
                .config
                .accent_color
                .clone()
                .map(|color| color.to_color_tuple())
                .map(|color| match color {
                    None => Err(fdo::Error::Failed(
                        "zenzai: could not parse color".to_string(),
                    )),
                    Some(color) => Ok(color.into()),
                })
                .unwrap(),
            _ => Err(fdo::Error::Failed(format!(
                "zenzai: unknown key: {}.{}",
                namespace, key
            ))),
        };

        tracing::debug!("respone) read: {}.{} = {:?}", namespace, key, value);

        value
    }

    async fn read_all(&self, namespaces: Vec<&str>) -> fdo::Result<Value<'_>> {
        tracing::debug!("read_all: {:?}", namespaces);

        if !namespaces.contains(&NAMESPACE) {
            return Err(fdo::Error::Failed(format!(
                "zenzai: unknown namespaces {:?}",
                namespaces
            )));
        }

        let mut result = HashMap::new();

        let mut nsmap = HashMap::new();

        let color_scheme = self
            .config
            .color_scheme
            .clone()
            .unwrap_or(ColorScheme::default());
        nsmap.insert(KEY_COLOR_SCHEME, Value::U32(color_scheme.clone().into()));

        let contrast = self.config.contrast.clone().unwrap_or(Contrast::default());
        nsmap.insert(KEY_CONTRAST, Value::U32(contrast.clone().into()));

        if let Some(color) = &self.config.accent_color {
            if let Some(color) = color.to_color_tuple() {
                nsmap.insert(KEY_ACCENT_COLOR, color.into());
            }
        }

        result.insert(NAMESPACE, nsmap);

        Ok(result.into())
    }

    #[zbus(signal)]
    pub async fn setting_changed(
        ctx: &SignalEmitter<'_>,
        namespace: String,
        key: String,
        value: Value<'_>,
    ) -> zbus::Result<()>;
}
