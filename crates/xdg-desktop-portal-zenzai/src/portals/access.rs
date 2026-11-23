use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::watch,
};
use zbus::{Connection, ObjectPath, OwnedVariant, Variant, fdo, interface, message::Header};
use zenzai_core::ipc::{
    AccessChoice, AccessRequest, AccessResponse, EXIT_CANCELLED, EXIT_SUCCESS,
    MAX_ACCESS_MESSAGE_BYTES,
};

pub const DIALOG_COMMAND: &str = "zenzai-dialog-access";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AccessConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

pub struct AccessService {
    pub command: watch::Receiver<PathBuf>,
}

struct Request {
    sender: String,
    cancelled: watch::Sender<bool>,
}

#[interface(name = "org.freedesktop.impl.portal.Request")]
impl Request {
    #[allow(clippy::needless_pass_by_value)]
    fn close(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        if header
            .sender()
            .is_none_or(|sender| sender.as_str() != self.sender)
        {
            return Err(fdo::Error::AccessDenied(
                "request belongs to another caller".into(),
            ));
        }

        self.cancelled.send_replace(true);
        Ok(())
    }

    #[allow(clippy::unused_self)]
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }
}

fn valid_selection(choice: &AccessChoice, selected: &str) -> bool {
    if choice.options.is_empty() {
        matches!(selected, "true" | "false")
    } else {
        choice.options.iter().any(|(id, _)| id == selected)
    }
}

fn validate_choices(choices: &[AccessChoice]) -> Result<()> {
    let mut ids = HashSet::new();
    for choice in choices {
        ensure!(
            !choice.id.is_empty() && !choice.label.is_empty(),
            "empty choice ID or label"
        );
        ensure!(ids.insert(&choice.id), "duplicate choice ID");
        let mut option_ids = HashSet::new();
        for (id, label) in &choice.options {
            ensure!(
                !id.is_empty() && !label.is_empty(),
                "empty option ID or label"
            );
            ensure!(option_ids.insert(id), "duplicate option ID");
        }

        ensure!(
            choice.selected.is_empty() || valid_selection(choice, &choice.selected),
            "invalid initial choice selection"
        );
    }

    Ok(())
}

fn validate_response(choices: &[AccessChoice], response: &AccessResponse) -> Result<()> {
    ensure!(
        response.choices.len() == choices.len(),
        "missing or extra choices"
    );
    let mut ids = HashSet::new();
    for (id, selected) in &response.choices {
        ensure!(ids.insert(id), "duplicate response choice ID");
        let choice = choices
            .iter()
            .find(|choice| choice.id == *id)
            .context("unknown response choice ID")?;
        ensure!(
            valid_selection(choice, selected),
            "invalid response choice selection"
        );
    }

    Ok(())
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

async fn run_dialog(
    command: PathBuf,
    request: &AccessRequest,
    mut cancelled: watch::Receiver<bool>,
) -> Result<Option<AccessResponse>> {
    let input = serde_json::to_vec(request)?;
    ensure!(
        input.len() as u64 <= MAX_ACCESS_MESSAGE_BYTES,
        "access dialog request too large"
    );
    let mut child = Command::new(command)
        .arg("--portal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("could not start access dialog")?;
    let result = tokio::select! {
        biased;
        _ = cancelled.wait_for(|cancelled| *cancelled) => Ok(None),
        result = async {
            let mut stdin = child.stdin.take().context("missing dialog stdin")?;
            stdin.write_all(&input).await?;
            drop(stdin);
            let mut output = Vec::new();
            child.stdout.take().context("missing dialog stdout")?.take(MAX_ACCESS_MESSAGE_BYTES + 1).read_to_end(&mut output).await?;
            ensure!(output.len() as u64 <= MAX_ACCESS_MESSAGE_BYTES, "access dialog response too large");
            let status = child.wait().await?;
            match status.code() {
                Some(EXIT_CANCELLED) => Ok(None),
                Some(EXIT_SUCCESS) => {
                    let response: AccessResponse = serde_json::from_slice(&output)?;
                    validate_response(&request.choices, &response)?;
                    Ok(Some(response))
                }
                _ => bail!("access dialog failed: {status}"),
            }
        } => result,
    };

    // Reap the child on cancellation, malformed output, and pipe errors too.
    let _ = child.kill().await;
    result
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
                modal: options
                    .get("modal")
                    .map(Variant::downcast_ref::<bool>)
                    .transpose()?
                    .unwrap_or(true),
                choices: choices_option(&options)?,
            })
        })()
        .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing caller".into()))?
            .to_string();
        let (cancelled, receiver) = watch::channel(false);
        if connection
            .object_server()
            .at(&handle, Request { sender, cancelled })
            .await?
            .is_some()
        {
            return Err(fdo::Error::InvalidArgs(
                "request handle already in use".into(),
            ));
        }

        let command = self.command.borrow().clone();
        let outcome = run_dialog(command, &request, receiver).await;
        connection
            .object_server()
            .remove::<Request, _>(&handle)
            .await?;
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
