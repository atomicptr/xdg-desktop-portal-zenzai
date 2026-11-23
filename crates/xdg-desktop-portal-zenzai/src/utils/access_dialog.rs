use std::{collections::HashSet, path::PathBuf, process::Stdio};

use anyhow::{Context, Result, bail, ensure};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::watch,
};
use zbus::{Connection, ObjectPath, fdo, interface, message::Header};
use zenzai_core::ipc::{
    AccessChoice, AccessRequest, AccessResponse, EXIT_CANCELLED, EXIT_SUCCESS,
    MAX_ACCESS_MESSAGE_BYTES,
};

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

pub fn validate_choices(choices: &[AccessChoice]) -> Result<()> {
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

pub fn validate_response(choices: &[AccessChoice], response: &AccessResponse) -> Result<()> {
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

pub async fn prompt(
    command: PathBuf,
    request: &AccessRequest,
    handle: ObjectPath<'_>,
    connection: &Connection,
    header: Header<'_>,
) -> fdo::Result<Result<Option<AccessResponse>>> {
    prompt_prepared(
        command,
        async { Ok((request.clone(), ())) },
        handle,
        connection,
        header,
    )
    .await
    .map(|outcome| outcome.map(|response| response.map(|(response, ())| response)))
}

pub async fn prompt_prepared<T>(
    command: PathBuf,
    prepare: impl Future<Output = Result<(AccessRequest, T)>>,
    handle: ObjectPath<'_>,
    connection: &Connection,
    header: Header<'_>,
) -> fdo::Result<Result<Option<(AccessResponse, T)>>> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("missing caller".into()))?
        .to_string();
    let (cancelled, mut receiver) = watch::channel(false);
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

    let outcome = async {
        let (request, value) = tokio::select! {
            biased;
            _ = receiver.wait_for(|cancelled| *cancelled) => return Ok(None),
            prepared = prepare => prepared?,
        };
        if *receiver.borrow() {
            return Ok(None);
        }
        run_dialog(command, &request, receiver)
            .await
            .map(|response| response.map(|response| (response, value)))
    }
    .await;
    connection
        .object_server()
        .remove::<Request, _>(&handle)
        .await?;

    Ok(outcome)
}
