use std::collections::HashMap;

use zbus::{Variant, fdo, interface, object_server::SignalEmitter};

use crate::utils::matches_glob;

use super::{
    config::SettingsConfig,
    constants::{KEY_ACCENT_COLOR, KEY_COLOR_SCHEME, KEY_CONTRAST, NAMESPACE},
};

type Namespaces = HashMap<String, HashMap<String, Variant<'static>>>;

pub struct SettingsService {
    pub config: SettingsConfig,
}

impl SettingsService {
    fn values(&self) -> Namespaces {
        let config = &self.config;

        let mut appearance = HashMap::from([
            (
                KEY_COLOR_SCHEME.to_string(),
                u32::from(config.color_scheme).into(),
            ),
            (KEY_CONTRAST.to_string(), u32::from(config.contrast).into()),
        ]);

        if let Some(color) = &config.accent_color {
            let color = color.color();
            let rgb = (f64::from(color.r), f64::from(color.g), f64::from(color.b));
            appearance.insert(KEY_ACCENT_COLOR.to_string(), rgb.into());
        }

        config
            .dict
            .iter()
            .map(|(namespace, values)| {
                let values = values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.into()));
                (namespace.clone(), values.collect())
            })
            .chain([(NAMESPACE.to_string(), appearance)])
            .collect()
    }
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsService {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        1
    }

    async fn read(&self, namespace: &str, key: &str) -> fdo::Result<Variant<'static>> {
        tracing::debug!("Read: {namespace}.{key}");

        self.values()
            .remove(namespace)
            .and_then(|mut values| values.remove(key))
            .ok_or_else(|| fdo::Error::Failed(format!("zenzai: unknown key {namespace}.{key}")))
    }

    async fn read_all(&self, namespaces: Vec<&str>) -> fdo::Result<Namespaces> {
        tracing::debug!("ReadAll: {namespaces:?}");

        let wanted = |namespace: &str| {
            namespaces.is_empty() || namespaces.iter().any(|p| matches_glob(p, namespace))
        };

        Ok(self
            .values()
            .into_iter()
            .filter(|(namespace, _)| wanted(namespace))
            .collect())
    }

    #[zbus(signal)]
    pub async fn setting_changed(
        ctx: &SignalEmitter<'_>,
        namespace: String,
        key: String,
        value: Variant<'_>,
    ) -> zbus::Result<()>;
}
