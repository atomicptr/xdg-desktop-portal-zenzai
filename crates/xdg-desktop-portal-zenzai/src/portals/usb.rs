use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::{Result, ensure};
use rust_i18n::t;
use serde::Deserialize;
use tokio::sync::watch;
use zbus::{Connection, ObjectPath, OwnedVariant, Variant, fdo, interface, message::Header};
use zenzai_core::ipc::{AccessChoice, AccessRequest, AccessResponse};

use crate::utils::access_dialog::{prompt, validate_response};

pub use super::access::DIALOG_COMMAND;

// D-Bus wire tuple: device ID, information, and requested access options.
type DeviceInput = (
    String,
    HashMap<String, OwnedVariant>,
    HashMap<String, OwnedVariant>,
);

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UsbConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

impl Default for UsbConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: None,
        }
    }
}

pub struct UsbService {
    pub command: watch::Receiver<PathBuf>,
}

#[derive(Debug)]
struct Device {
    id: String,
    label: String,
    writable: bool,
}

fn text_property<'a>(
    properties: &'a HashMap<String, OwnedVariant>,
    key: &str,
) -> Result<Option<&'a str>> {
    properties
        .get(key)
        .map(<&str>::try_from)
        .transpose()
        .map_err(Into::into)
}

fn parse_devices(devices: Vec<DeviceInput>) -> Result<Vec<Device>> {
    ensure!(!devices.is_empty(), "no USB devices requested");
    let mut ids = HashSet::new();
    devices
        .into_iter()
        .map(|(id, info, options)| {
            ensure!(
                !id.is_empty() && ids.insert(id.clone()),
                "empty or duplicate USB device ID"
            );
            let writable = options
                .get("writable")
                .map(bool::try_from)
                .transpose()?
                .unwrap_or(false);
            let properties = info
                .get("properties")
                .map(|value| HashMap::<String, OwnedVariant>::try_from(value.try_clone()?))
                .transpose()?
                .unwrap_or_default();
            let vendor = text_property(&properties, "ID_VENDOR_FROM_DATABASE")?
                .or(text_property(&properties, "ID_VENDOR")?)
                .or(text_property(&properties, "ID_VENDOR_ID")?)
                .unwrap_or("");
            let model = text_property(&properties, "ID_MODEL_FROM_DATABASE")?
                .or(text_property(&properties, "ID_MODEL")?)
                .or(text_property(&properties, "ID_MODEL_ID")?)
                .unwrap_or("");
            let name = format!("{vendor} {model}");
            let mode = if writable {
                t!("usb.read-write")
            } else {
                t!("usb.read-only")
            };
            // The opaque ID distinguishes identical devices without exposing serial numbers.
            let label = format!("{} ({id}) — {mode}", name.trim());
            Ok(Device {
                id,
                label,
                writable,
            })
        })
        .collect()
}

fn request(app_id: &str, parent_window: &str, devices: &[Device]) -> AccessRequest {
    AccessRequest {
        app_id: app_id.into(),
        parent_window: parent_window.into(),
        title: t!("usb.allow-access").into(),
        subtitle: t!(
            "usb.requested-by",
            app = if app_id.is_empty() {
                t!("usb.unknown-application").into_owned()
            } else {
                app_id.to_owned()
            }
        )
        .into(),
        body: t!("usb.select-devices").into(),
        deny_label: Some(t!("common.deny").into()),
        grant_label: Some(t!("usb.allow-selected").into()),
        icon: Some("drive-removable-media-usb".into()),
        image: None,
        modal: true,
        choices: devices
            .iter()
            .map(|device| AccessChoice {
                id: device.id.clone(),
                label: device.label.clone(),
                options: Vec::new(),
                selected: "false".into(),
            })
            .collect(),
    }
}

fn approved_devices(
    devices: &[Device],
    request: &AccessRequest,
    response: &AccessResponse,
) -> Result<Vec<(String, HashMap<String, OwnedVariant>)>> {
    validate_response(&request.choices, response)?;
    Ok(devices
        .iter()
        .filter(|device| {
            response
                .choices
                .iter()
                .any(|(id, selected)| id == &device.id && selected == "true")
        })
        .map(|device| {
            (
                device.id.clone(),
                HashMap::from([("writable".into(), OwnedVariant::from(device.writable))]),
            )
        })
        .collect())
}

#[interface(name = "org.freedesktop.impl.portal.Usb")]
impl UsbService {
    #[allow(clippy::unused_self)]
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    #[allow(clippy::too_many_arguments)]
    async fn acquire_devices(
        &self,
        handle: ObjectPath<'_>,
        parent_window: &str,
        app_id: &str,
        devices: Vec<DeviceInput>,
        options: HashMap<String, OwnedVariant>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        drop(options);

        let devices =
            parse_devices(devices).map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        let request = request(app_id, parent_window, &devices);
        let command = self.command.borrow().clone();
        let outcome = prompt(command, &request, handle, connection, header).await?;
        let outcome = outcome.and_then(|response| {
            response
                .map(|response| approved_devices(&devices, &request, &response))
                .transpose()
        });
        match outcome {
            Ok(Some(devices)) if !devices.is_empty() => {
                let devices = OwnedVariant::try_from(Variant::from(devices))
                    .map_err(|err| fdo::Error::Failed(err.to_string()))?;
                Ok((0, HashMap::from([("devices".into(), devices)])))
            }
            Ok(_) => Ok((1, HashMap::new())),
            Err(err) => {
                tracing::error!("USB dialog: {err:#}");
                Ok((2, HashMap::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_is_a_subset_with_original_access_modes() {
        let input = || {
            vec![
                ("read".into(), HashMap::new(), HashMap::new()),
                (
                    "write".into(),
                    HashMap::new(),
                    HashMap::from([("writable".into(), OwnedVariant::from(true))]),
                ),
            ]
        };
        let devices = parse_devices(input()).unwrap();
        let request = request("test.app", "", &devices);
        assert!(
            request
                .choices
                .iter()
                .all(|choice| choice.selected == "false")
        );
        let mut response = AccessResponse {
            choices: vec![
                ("write".into(), "true".into()),
                ("read".into(), "false".into()),
            ],
        };
        let approved = approved_devices(&devices, &request, &response).unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].0, "write");
        assert!(bool::try_from(&approved[0].1["writable"]).unwrap());
        response.choices[1].1 = "true".into();
        let approved = approved_devices(&devices, &request, &response).unwrap();
        assert!(!bool::try_from(&approved[0].1["writable"]).unwrap());
        assert_eq!(Variant::from(approved).value_signature(), "a(sa{sv})");
        response.choices[0].0 = "injected".into();
        assert!(approved_devices(&devices, &request, &response).is_err());
        response.choices[0].0 = "read".into();
        assert!(approved_devices(&devices, &request, &response).is_err());
        let mut duplicate = input();
        duplicate[1].0 = "read".into();
        assert!(parse_devices(duplicate).is_err());
        let mut malformed = input();
        malformed[0]
            .2
            .insert("writable".into(), OwnedVariant::from(1_u32));
        assert!(parse_devices(malformed).is_err());
    }
}
