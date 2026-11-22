use std::{
    fs::File,
    io::{self, Read},
    os::fd::OwnedFd,
    path::Path,
    process::Stdio,
};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;
use tokio::{io::AsyncWriteExt, process::Command};
use zbus::Fd;
use zenzai_core::ipc::{EXIT_CANCELLED, EXIT_SUCCESS, PrintRequest, PrintResponse};

pub const DIALOG_COMMAND: &str = "zenzai-dialog-print";

const PORTAL_FLAG: &str = "--portal";
const TEMPFILE_PREFIX: &str = "zenzai-print-";

// applications can hand us arbitrary fds, cap how much we buffer at the limit cups_rs accepts
const MAX_DOCUMENT_SIZE: u64 = 100 * 1024 * 1024;

pub enum Outcome {
    Accepted(PrintResponse),
    Cancelled,
}

pub async fn run(command: &Path, request: &PrintRequest) -> Result<Outcome> {
    let mut child = Command::new(command)
        .arg(PORTAL_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not start {}", command.display()))?;

    let mut stdin = child.stdin.take().context("could not open dialog stdin")?;
    stdin.write_all(&serde_json::to_vec(request)?).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;

    match output.status.code() {
        Some(EXIT_SUCCESS) => Ok(Outcome::Accepted(serde_json::from_slice(&output.stdout)?)),
        Some(EXIT_CANCELLED) => Ok(Outcome::Cancelled),
        status => bail!("print dialog failed with status {status:?}"),
    }
}

pub async fn copy_document(fd: Fd<'_>) -> Result<NamedTempFile> {
    let fd = OwnedFd::try_from(fd)?;

    tokio::task::spawn_blocking(move || copy_to_tempfile(fd)).await?
}

fn copy_to_tempfile(fd: OwnedFd) -> Result<NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .prefix(TEMPFILE_PREFIX)
        .tempfile()?;

    let copied = io::copy(
        &mut File::from(fd).take(MAX_DOCUMENT_SIZE + 1),
        file.as_file_mut(),
    )?;

    if copied > MAX_DOCUMENT_SIZE {
        bail!("document exceeds {MAX_DOCUMENT_SIZE} bytes");
    }

    Ok(file)
}
