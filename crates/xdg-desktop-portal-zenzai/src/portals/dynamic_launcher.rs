use std::{collections::HashMap, path::PathBuf, process::Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::watch,
};
use zbus::{
    Array, Connection, ObjectPath, OwnedVariant, Str, Structure, Variant, fdo, interface,
    message::Header,
};
use zenzai_core::ipc::{
    EXIT_CANCELLED, EXIT_SUCCESS, LauncherRequest, LauncherResponse, LauncherType,
    MAX_LAUNCHER_ICON_BYTES as MAX_ICON_BYTES, MAX_LAUNCHER_MESSAGE_BYTES as MAX_REPLY_BYTES,
};

pub const DIALOG_COMMAND: &str = "zenzai-dialog-launcher";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DynamicLauncherConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

pub struct DynamicLauncherService {
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

fn icon_bytes(icon: &Variant<'_>) -> Result<Vec<u8>> {
    let structure = icon.downcast_ref::<&Structure<'_>>()?;
    let fields = structure.fields();
    ensure!(fields.len() == 2, "expected a serialized GBytesIcon");
    ensure!(
        fields[0].downcast_ref::<&str>()? == "bytes",
        "expected a serialized GBytesIcon"
    );
    let data = fields[1].downcast_ref::<&Array<'_>>()?;
    ensure!(
        !data.is_empty() && data.len() <= MAX_ICON_BYTES,
        "invalid icon size"
    );
    data.inner()
        .iter()
        .map(|value| Ok(value.downcast_ref::<u8>()?))
        .collect()
}

fn boolean(options: &HashMap<&str, Variant<'_>>, key: &str, default: bool) -> Result<bool> {
    options
        .get(key)
        .map_or(Ok(default), |value| Ok(value.downcast_ref::<bool>()?))
}

fn request(
    app_id: &str,
    name: &str,
    parent_window: &str,
    icon: &Variant<'_>,
    options: &HashMap<&str, Variant<'_>>,
) -> Result<LauncherRequest> {
    let launcher_type = match options
        .get("launcher_type")
        .map(Variant::downcast_ref::<u32>)
        .transpose()?
        .unwrap_or(1)
    {
        1 => LauncherType::Application,
        2 => LauncherType::Webapp,
        _ => bail!("unsupported launcher type"),
    };
    let target = options
        .get("target")
        .map(|v| v.downcast_ref::<&str>().map(str::to_owned))
        .transpose()?;

    Ok(LauncherRequest {
        app_id: app_id.into(),
        name: name.into(),
        launcher_type,
        target,
        icon: icon_bytes(icon)?,
        editable_name: boolean(options, "editable_name", true)?,
        editable_icon: boolean(options, "editable_icon", false)?,
        parent_window: parent_window.into(),
        modal: boolean(options, "modal", true)?,
    })
}

async fn run_dialog(
    command: PathBuf,
    request: &LauncherRequest,
    mut cancelled: watch::Receiver<bool>,
) -> Result<Option<LauncherResponse>> {
    let mut child = Command::new(command)
        .arg("--portal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("could not start launcher dialog")?;
    let result = tokio::select! {
        biased;
        _ = cancelled.wait_for(|cancelled| *cancelled) => Ok(None),
        result = async {
            let mut stdin = child.stdin.take().context("missing dialog stdin")?;
            stdin.write_all(&serde_json::to_vec(request)?).await?;
            drop(stdin);
            let mut output = Vec::new();
            child.stdout.take().context("missing dialog stdout")?
                .take(MAX_REPLY_BYTES + 1).read_to_end(&mut output).await?;
            ensure!(output.len() as u64 <= MAX_REPLY_BYTES, "launcher dialog response too large");
            let status = child.wait().await?;
            match status.code() {
                Some(EXIT_CANCELLED) => Ok(None),
                Some(EXIT_SUCCESS) => {
                    let mut response: LauncherResponse = serde_json::from_slice(&output)?;
                    if !request.editable_name {
                        response.name.clone_from(&request.name);
                    }

                    if !request.editable_icon {
                        response.icon = None;
                    }

                    ensure!(!response.name.trim().is_empty(), "empty launcher name");
                    if let Some(icon) = &response.icon {
                        ensure!(!icon.is_empty() && icon.len() <= MAX_ICON_BYTES, "invalid icon size");
                    }

                    Ok(Some(response))
                }
                _ => bail!("launcher dialog failed: {status}"),
            }
        } => result,
    };

    // Also reap the child on cancellation, malformed output, and pipe errors.
    let _ = child.kill().await;
    result
}

#[interface(name = "org.freedesktop.impl.portal.DynamicLauncher")]
impl DynamicLauncherService {
    #[allow(clippy::unused_self)]
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    #[allow(clippy::unused_self)]
    #[zbus(property)]
    fn supported_launcher_types(&self) -> u32 {
        3
    }

    #[allow(clippy::unused_self)]
    fn request_install_token(&self, app_id: &str, options: HashMap<&str, Variant<'_>>) -> u32 {
        let _ = (app_id, options);
        // No caller may bypass user confirmation.
        2
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_install(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        name: &str,
        icon_v: Variant<'_>,
        options: HashMap<&str, Variant<'_>>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let request = request(app_id, name, parent_window, &icon_v, &options)
            .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing caller".into()))?
            .to_string();
        let icon = result_icon(icon_v).map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
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
                let icon = match response.icon {
                    Some(bytes) => result_icon(Variant::from(("bytes", Variant::from(bytes))))
                        .map_err(|err| fdo::Error::Failed(err.to_string()))?,
                    None => icon,
                };

                Ok((
                    0,
                    HashMap::from([
                        ("name".into(), OwnedVariant::from(Str::from(response.name))),
                        ("icon".into(), icon),
                    ]),
                ))
            }
            Ok(None) => Ok((1, HashMap::new())),
            Err(err) => {
                tracing::error!("launcher dialog: {err:#}");
                Ok((2, HashMap::new()))
            }
        }
    }
}

// The icon result is itself a variant, inside the vardict value wrapper.
fn result_icon(icon: Variant<'_>) -> Result<OwnedVariant> {
    Ok(OwnedVariant::try_from(Variant::Variant(Box::new(icon)))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_serialized_bytes_icons_are_accepted() {
        let bytes = vec![1_u8, 2, 3];
        let icon = Variant::from(("bytes", Variant::from(bytes.clone())));
        assert_eq!(icon_bytes(&icon).unwrap(), bytes);
        let result = result_icon(icon).unwrap();
        assert_eq!(result.value_signature(), "v");
        assert!(matches!(&*result, Variant::Variant(inner) if inner.value_signature() == "(sv)"));
        assert_eq!(icon_bytes(&result).unwrap(), bytes);
        assert!(icon_bytes(&Variant::from(("file", Variant::from("/tmp/icon")))).is_err());
        assert!(icon_bytes(&Variant::from(("bytes", Variant::from(Vec::<u8>::new())))).is_err());
        assert!(icon_bytes(&Variant::from("invalid")).is_err());
    }
}
