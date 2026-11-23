use std::{collections::HashMap, path::PathBuf};

#[cfg(test)]
use crate::utils::access_dialog::validate_response;
use crate::utils::access_dialog::{prompt, validate_choices};
use anyhow::Result;
use serde::Deserialize;
use tokio::sync::watch;
use zbus::{Connection, ObjectPath, OwnedVariant, Variant, fdo, interface, message::Header};
#[cfg(test)]
use zenzai_core::ipc::AccessResponse;
use zenzai_core::ipc::{AccessChoice, AccessRequest};

pub const DIALOG_COMMAND: &str = "zenzai-dialog-access";

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AccessConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

impl Default for AccessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: None,
        }
    }
}

pub struct AccessService {
    pub command: watch::Receiver<PathBuf>,
}

fn text_option(options: &HashMap<&str, Variant<'_>>, key: &str) -> Result<Option<String>> {
    options
        .get(key)
        .map(|value| value.downcast_ref::<&str>().map(String::from))
        .transpose()
        .map_err(Into::into)
}

fn choices_option(options: &HashMap<&str, Variant<'_>>) -> Result<Vec<AccessChoice>> {
    let Some(value) = options.get("choices") else {
        return Ok(Vec::new());
    };
    let mut choices = value
        .try_clone()?
        .downcast::<Vec<(String, String, Vec<(String, String)>, String)>>()?
        .into_iter()
        .map(|(id, label, options, selected)| AccessChoice {
            id,
            label,
            options,
            selected,
        })
        .collect::<Vec<_>>();
    validate_choices(&choices)?;
    for choice in &mut choices {
        if choice.selected.is_empty() {
            choice.selected = choice
                .options
                .first()
                .map_or_else(|| "false".into(), |(id, _)| id.clone());
        }
    }
    Ok(choices)
}

#[interface(name = "org.freedesktop.impl.portal.Access")]
impl AccessService {
    #[allow(clippy::too_many_arguments)]
    async fn access_dialog(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        subtitle: &str,
        body: &str,
        options: HashMap<&str, Variant<'_>>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let request = (|| -> Result<AccessRequest> {
            Ok(AccessRequest {
                app_id: app_id.into(),
                parent_window: parent_window.into(),
                title: title.into(),
                subtitle: subtitle.into(),
                body: body.into(),
                deny_label: text_option(&options, "deny_label")?,
                grant_label: text_option(&options, "grant_label")?,
                icon: text_option(&options, "icon")?,
                image: None,
                modal: options
                    .get("modal")
                    .map(Variant::downcast_ref::<bool>)
                    .transpose()?
                    .unwrap_or(true),
                choices: choices_option(&options)?,
            })
        })()
        .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        let command = self.command.borrow().clone();
        let outcome = prompt(command, &request, handle, connection, header).await?;
        match outcome {
            Ok(Some(response)) => {
                let choices = OwnedVariant::try_from(Variant::from(response.choices))
                    .map_err(|err| fdo::Error::Failed(err.to_string()))?;
                Ok((0, HashMap::from([("choices".into(), choices)])))
            }
            Ok(None) => Ok((1, HashMap::new())),
            Err(err) => {
                tracing::error!("access dialog: {err:#}");
                Ok((2, HashMap::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices() -> Vec<AccessChoice> {
        vec![
            AccessChoice {
                id: "toggle".into(),
                label: "Toggle".into(),
                options: vec![],
                selected: "false".into(),
            },
            AccessChoice {
                id: "mode".into(),
                label: "Mode".into(),
                options: vec![("one".into(), "One".into())],
                selected: String::new(),
            },
        ]
    }

    #[test]
    fn validates_initial_choices() {
        let mut choices = choices();
        assert!(validate_choices(&choices).is_ok());
        choices[0].selected.clear();
        let serialized = choices
            .iter()
            .map(|choice| {
                (
                    choice.id.clone(),
                    choice.label.clone(),
                    choice.options.clone(),
                    choice.selected.clone(),
                )
            })
            .collect::<Vec<_>>();
        let options = HashMap::from([("choices", Variant::from(serialized))]);
        assert_eq!(options["choices"].value_signature(), "a(ssa(ss)s)");
        let defaults = choices_option(&options).unwrap();
        assert_eq!(defaults[0].selected, "false");
        assert_eq!(defaults[1].selected, "one");
        let response = AccessResponse {
            choices: defaults
                .iter()
                .map(|choice| (choice.id.clone(), choice.selected.clone()))
                .collect(),
        };
        assert!(validate_response(&defaults, &response).is_ok());
        assert!(
            choices_option(&HashMap::from([("choices", Variant::from("wrong type"))])).is_err()
        );
        choices[0].selected = "yes".into();
        assert!(validate_choices(&choices).is_err());
        choices[0].selected.clear();
        choices[1].options.push(("one".into(), "Duplicate".into()));
        assert!(validate_choices(&choices).is_err());
        choices[1].options.pop();
        choices[1].id = "toggle".into();
        assert!(validate_choices(&choices).is_err());
    }

    #[test]
    fn requires_complete_valid_response() {
        let choices = choices();
        let mut response = AccessResponse {
            choices: vec![
                ("mode".into(), "one".into()),
                ("toggle".into(), "true".into()),
            ],
        };
        assert!(validate_response(&choices, &response).is_ok());
        response.choices[1].1.clear();
        assert!(validate_response(&choices, &response).is_err());
        response.choices[1] = response.choices[0].clone();
        assert!(validate_response(&choices, &response).is_err());
        response.choices.pop();
        assert!(validate_response(&choices, &response).is_err());
        response.choices.push(("unknown".into(), "true".into()));
        assert!(validate_response(&choices, &response).is_err());
    }
}
