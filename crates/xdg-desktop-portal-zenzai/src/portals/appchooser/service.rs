use std::{collections::HashMap, path::PathBuf, process::Stdio};

use anyhow::{Context, Result, bail};
use tokio::{io::AsyncWriteExt, process::Command, sync::watch};
use zbus::{ObjectPath, OwnedVariant, Variant, fdo, interface};
use zenzai_core::ipc::{AppSelectRequest, EXIT_CANCELLED, EXIT_SUCCESS};

pub const SELECT_COMMAND: &str = "zenzai-app-select";

pub struct AppChooserService {
    pub command: watch::Receiver<PathBuf>,
}

impl AppChooserService {
    pub async fn choose(&self, request: &AppSelectRequest) -> Result<u32> {
        let command = self.command.borrow().clone();
        let mut child = Command::new(&command)
            .arg("--portal")
            .stdin(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("could not start {}", command.display()))?;

        tracing::debug!(
            command = %command.display(),
            pid = child.id(),
            "started application selector"
        );

        let mut stdin = child.stdin.take().context("could not open chooser stdin")?;
        stdin.write_all(&serde_json::to_vec(request)?).await?;
        drop(stdin);

        let status = child.wait().await?;
        match status.code() {
            Some(EXIT_SUCCESS) => Ok(0),
            Some(EXIT_CANCELLED) => Ok(1),
            status => bail!("application chooser failed with status {status:?}"),
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.AppChooser")]
impl AppChooserService {
    #[tracing::instrument(level = "debug", skip_all, fields(request = %handle, app_id = %app_id))]
    async fn choose_application(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        choices: Vec<&str>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let _ = parent_window;
        tracing::debug!(
            choices = choices.len(),
            option_keys = ?options.keys().collect::<Vec<_>>(),
            "received application chooser request"
        );

        let option = |key| {
            options
                .get(key)
                .and_then(|value| value.downcast_ref::<&str>().ok())
                .with_context(|| format!("missing option {key}"))
        };
        let result = async {
            let request = AppSelectRequest {
                uri: option("uri")?.to_string(),
                content_type: option("content_type")?.to_string(),
                choices: choices.into_iter().map(str::to_string).collect(),
                activation_token: option("activation_token").ok().map(str::to_string),
                ..Default::default()
            };
            self.choose(&request).await
        }
        .await;

        match result {
            // Preserve the existing backend-owned launch flow, including custom commands.
            Ok(response) => {
                tracing::debug!(response, "application chooser request completed");
                Ok((response, HashMap::new()))
            }
            Err(err) => {
                tracing::error!("{err:#}");
                Err(fdo::Error::Failed(format!("{err:#}")))
            }
        }
    }
}
