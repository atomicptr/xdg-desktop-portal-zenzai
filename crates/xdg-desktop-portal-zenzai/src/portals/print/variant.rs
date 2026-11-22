use std::collections::HashMap;

use zbus::{OwnedVariant, Str, Variant};
use zenzai_core::ipc::{PageSetup, PrintSettings, SetupValue};

// the portal spec defines all print settings as strings
pub fn settings_from_dict(dict: &HashMap<&str, Variant<'_>>) -> PrintSettings {
    dict.iter()
        .filter_map(|(key, value)| {
            let value: &str = value.downcast_ref().ok()?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

pub fn setup_from_dict(dict: &HashMap<&str, Variant<'_>>) -> PageSetup {
    dict.iter()
        .filter_map(|(key, value)| {
            let value = match value.downcast_ref::<f64>() {
                Ok(number) => SetupValue::Number(number),
                Err(_) => SetupValue::Text(value.downcast_ref::<&str>().ok()?.to_string()),
            };

            Some((key.to_string(), value))
        })
        .collect()
}

// values must be wrapped in variants, applications expect an `a{sv}` vardict
pub fn settings_to_variant(settings: &PrintSettings) -> OwnedVariant {
    settings
        .iter()
        .map(|(key, value)| (key.clone(), OwnedVariant::from(Str::from(value.clone()))))
        .collect::<HashMap<_, _>>()
        .into()
}

pub fn setup_to_variant(setup: &PageSetup) -> OwnedVariant {
    setup
        .iter()
        .map(|(key, value)| {
            let value = match value {
                SetupValue::Number(number) => OwnedVariant::from(*number),
                SetupValue::Text(text) => OwnedVariant::from(Str::from(text.clone())),
            };

            (key.clone(), value)
        })
        .collect::<HashMap<_, _>>()
        .into()
}

pub fn get_bool(options: &HashMap<&str, Variant<'_>>, key: &str) -> bool {
    options
        .get(key)
        .and_then(|value| value.downcast_ref::<bool>().ok())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_serialized_as_vardict() {
        let settings = PrintSettings::from([("n-copies".to_string(), "2".to_string())]);

        assert_eq!(settings_to_variant(&settings).value_signature(), "a{sv}");
    }
}
