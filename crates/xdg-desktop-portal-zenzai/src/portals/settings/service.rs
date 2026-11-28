use std::{collections::HashMap, mem};

use zbus::{Variant, fdo, interface, object_server::SignalEmitter};

use crate::utils::matches_glob;

use super::{
    config::SettingsConfig,
    constants::{KEY_ACCENT_COLOR, KEY_COLOR_SCHEME, KEY_CONTRAST, NAMESPACE},
};

type Namespaces = HashMap<String, HashMap<String, Variant<'static>>>;

pub struct SettingChange {
    pub namespace: String,
    pub key: String,
    pub value: Variant<'static>,
}

pub struct SettingsService {
    values: Namespaces,
}

impl SettingsService {
    pub fn new(config: SettingsConfig) -> Self {
        Self {
            values: Self::values(config),
        }
    }

    pub fn update(&mut self, config: SettingsConfig) -> Vec<SettingChange> {
        let previous = mem::replace(&mut self.values, Self::values(config));

        self.values
            .iter()
            .flat_map(|(namespace, values)| {
                let previous = previous.get(namespace);
                values.iter().filter_map(move |(key, value)| {
                    if previous.and_then(|values| values.get(key)) == Some(value) {
                        return None;
                    }

                    Some(SettingChange {
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                    })
                })
            })
            .collect()
    }

    fn values(config: SettingsConfig) -> Namespaces {
        let mut appearance = HashMap::from([
            (
                KEY_COLOR_SCHEME.to_string(),
                u32::from(config.color_scheme).into(),
            ),
            (KEY_CONTRAST.to_string(), u32::from(config.contrast).into()),
        ]);

        // The Settings specification defines out-of-range RGB values as unset.
        let rgb = config
            .accent_color
            .as_ref()
            .map_or((-1.0, -1.0, -1.0), |color| {
                let color = color.color();
                (f64::from(color.r), f64::from(color.g), f64::from(color.b))
            });
        appearance.insert(KEY_ACCENT_COLOR.to_string(), rgb.into());

        config
            .dict
            .into_iter()
            .map(|(namespace, values)| {
                let values = values
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect();
                (namespace, values)
            })
            .chain([(NAMESPACE.to_string(), appearance)])
            .collect()
    }
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsService {
    // D-Bus properties require an instance receiver even for constant values.
    #[allow(clippy::unused_self)]
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    fn read(&self, namespace: &str, key: &str) -> fdo::Result<Variant<'static>> {
        tracing::debug!(namespace, key, "settings read");

        self.values
            .get(namespace)
            .and_then(|values| values.get(key))
            .cloned()
            .ok_or_else(|| fdo::Error::Failed(format!("zenzai: unknown key {namespace}.{key}")))
    }

    // zbus deserializes the D-Bus array into an owned vector.
    #[allow(clippy::needless_pass_by_value)]
    fn read_all(&self, namespaces: Vec<&str>) -> Namespaces {
        tracing::debug!(?namespaces, "settings read-all");

        let wanted = |namespace: &str| {
            namespaces.is_empty() || namespaces.iter().any(|p| matches_glob(p, namespace))
        };

        self.values
            .iter()
            .filter(|(namespace, _)| wanted(namespace))
            .map(|(namespace, values)| (namespace.clone(), values.clone()))
            .collect()
    }

    #[zbus(signal)]
    pub async fn setting_changed(
        ctx: &SignalEmitter<'_>,
        namespace: String,
        key: String,
        value: Variant<'_>,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_reports_only_effective_changes_and_resets_removed_appearance() {
        let initial = r##"
enabled = true
color-scheme = "dark"
accent-color = "#b4befe"
[dict."org.example"]
unchanged = true
changed = 1
removed = "old"
"##;
        let updated = r#"
enabled = true
color-scheme = "light"
[dict."org.example"]
unchanged = true
changed = 2
added = "new"
huge = 4294967296
"#;
        let mut service = SettingsService::new(toml::from_str(initial).unwrap());
        assert!(service.update(toml::from_str(initial).unwrap()).is_empty());

        let changes = service.update(toml::from_str(updated).unwrap());
        assert_eq!(changes.len(), 5);
        let find = |namespace, key| {
            &changes
                .iter()
                .find(|change| change.namespace == namespace && change.key == key)
                .unwrap()
                .value
        };
        assert_eq!(find(NAMESPACE, KEY_COLOR_SCHEME), &Variant::from(2u32));
        assert_eq!(
            find(NAMESPACE, KEY_ACCENT_COLOR),
            &Variant::from((-1.0, -1.0, -1.0))
        );
        assert_eq!(find("org.example", "changed"), &Variant::from(2i32));
        assert_eq!(find("org.example", "added"), &Variant::from("new"));
        assert_eq!(
            find("org.example", "huge"),
            &Variant::from(4_294_967_296i64)
        );
        assert!(!service.values["org.example"].contains_key("removed"));
        assert!(service.update(toml::from_str(updated).unwrap()).is_empty());

        let changes = service.update(SettingsConfig::default());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, KEY_COLOR_SCHEME);
        assert_eq!(changes[0].value, Variant::from(0u32));
    }
}
