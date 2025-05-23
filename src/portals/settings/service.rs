use std::collections::HashMap;

use zbus::{fdo, interface, object_server::SignalEmitter};
use zvariant::{OwnedValue, Value};

use crate::portals::settings::config::{Contrast, SettingsMapValue};
use crate::utils::hashmap::wildcard_get_all;

use super::config::{ColorScheme, SettingsConfig};

use super::constants::{KEY_ACCENT_COLOR, KEY_COLOR_SCHEME, KEY_CONTRAST, NAMESPACE};

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
        tracing::debug!("Read: {}.{}", namespace, key);

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

    async fn read_all(&self, namespaces: Vec<&str>) -> fdo::Result<OwnedValue> {
        tracing::debug!("ReadAll: {:?}", namespaces);

        // build the data map
        let mut m = HashMap::new();

        for (ns, map) in self.config.dict.clone().unwrap_or_default() {
            let mut nsmap = HashMap::new();

            for (k, v) in map {
                nsmap.insert(
                    k.clone(),
                    match v {
                        SettingsMapValue::String(str) => zvariant::Value::Str(str.into()),
                        SettingsMapValue::Int(int) => zvariant::Value::I64(int.clone()),
                        SettingsMapValue::Bool(b) => zvariant::Value::Bool(b.clone()),
                        SettingsMapValue::Float(f) => zvariant::Value::F64(f.clone()),
                    },
                );
            }

            m.insert(ns.clone(), nsmap);
        }

        let mut nsmap = HashMap::new();

        let color_scheme = self
            .config
            .color_scheme
            .clone()
            .unwrap_or(ColorScheme::default());
        nsmap.insert(
            KEY_COLOR_SCHEME.to_string(),
            Value::U32(color_scheme.clone().into()),
        );

        let contrast = self.config.contrast.clone().unwrap_or(Contrast::default());
        nsmap.insert(
            KEY_CONTRAST.to_string(),
            Value::U32(contrast.clone().into()),
        );

        if let Some(color) = &self.config.accent_color {
            if let Some(color) = color.to_color_tuple() {
                nsmap.insert(KEY_ACCENT_COLOR.to_string(), color.into());
            }
        }

        m.insert(NAMESPACE.to_string(), nsmap);

        // now lets query it

        // no namespaces set == get all data
        if namespaces.is_empty() {
            return Ok(m.into());
        }

        // execute the actual query
        let mut resmap = HashMap::new();

        for ns in namespaces {
            for (k, v) in wildcard_get_all(&m, ns.to_string()) {
                resmap.insert(k, v);
            }
        }

        Ok(resmap.into())
    }

    #[zbus(signal)]
    pub async fn setting_changed(
        ctx: &SignalEmitter<'_>,
        namespace: String,
        key: String,
        value: Value<'_>,
    ) -> zbus::Result<()>;
}
