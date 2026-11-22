use std::{
    fs::File,
    io::{self, Read},
    os::fd::OwnedFd,
    path::Path,
    process::Stdio,
};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::oneshot,
    time,
};
use zbus::Fd;
use zenzai_core::ipc::{EXIT_CANCELLED, EXIT_SUCCESS, PrintAction, PrintRequest, PrintResponse};

use super::tokens::TOKEN_LIFETIME;

pub const DIALOG_COMMAND: &str = "zenzai-dialog-print";

const PORTAL_FLAG: &str = "--portal";
const TEMPFILE_PREFIX: &str = "zenzai-print-";

// applications can hand us arbitrary fds, cap how much we buffer at the limit cups_rs accepts
const MAX_DOCUMENT_SIZE: u64 = 100 * 1024 * 1024;

pub struct PreparedPrint {
    pub response: PrintResponse,
    pub session: Option<PreviewSession>,
}

pub enum Outcome {
    Accepted(PreparedPrint),
    Cancelled,
}

pub struct PreviewSession {
    resume: oneshot::Sender<ResumeRequest>,
}

struct ResumeRequest {
    request: PrintRequest,
    document: NamedTempFile,
    completion: oneshot::Sender<Result<Outcome>>,
}

impl PreviewSession {
    pub async fn resume(self, request: PrintRequest, document: NamedTempFile) -> Result<Outcome> {
        let (completion, result) = oneshot::channel();
        let resume = ResumeRequest {
            request,
            document,
            completion,
        };

        if self.resume.send(resume).is_err() {
            return Ok(Outcome::Cancelled);
        }

        result.await.unwrap_or(Ok(Outcome::Cancelled))
    }
}

pub async fn run(command: &Path, request: &PrintRequest) -> Result<Outcome> {
    let mut child = Command::new(command)
        .arg(PORTAL_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not start {}", command.display()))?;

    tracing::debug!(
        command = %command.display(),
        pid = child.id(),
        "started print dialog"
    );

    let (completion, result) = oneshot::channel();
    let mut request = request.clone();

    // The task owns and reaps the child even if its caller or stored session is dropped.
    tokio::spawn(async move {
        let mut completion = Some(completion);
        let mut document = None;
        let outcome: Result<Outcome> = async {
            let mut stdin = child.stdin.take().context("could not open dialog stdin")?;
            let stdout = child
                .stdout
                .take()
                .context("could not open dialog stdout")?;
            let mut stdout = BufReader::new(stdout);
            let allow_preview = request.document.is_none() && request.interactive;

            for resumed in [false, true] {
                let mut payload = serde_json::to_vec(&request)?;
                payload.push(b'\n');
                let mut line = String::new();
                let reply = completion.as_mut().context("missing dialog completion")?;
                let read = tokio::select! {
                    result = async {
                        stdin.write_all(&payload).await?;
                        stdin.flush().await?;
                        stdout.read_line(&mut line).await
                    } => result?,
                    () = reply.closed() => return Ok(Outcome::Cancelled),
                };

                if read == 0 {
                    let status = tokio::select! {
                        status = child.wait() => status?,
                        () = reply.closed() => return Ok(Outcome::Cancelled),
                    };

                    if status.code() == Some(EXIT_CANCELLED) {
                        return Ok(Outcome::Cancelled);
                    }

                    bail!("print dialog exited without a response: {status}");
                }

                let response: PrintResponse = serde_json::from_str(&line)?;

                if response.action == PrintAction::Submit {
                    drop(stdin);
                    let status = tokio::select! {
                        status = child.wait() => status?,
                        () = reply.closed() => return Ok(Outcome::Cancelled),
                    };

                    return match status.code() {
                        Some(EXIT_SUCCESS) => Ok(Outcome::Accepted(PreparedPrint {
                            response,
                            session: None,
                        })),
                        Some(EXIT_CANCELLED) => Ok(Outcome::Cancelled),
                        status => bail!("print dialog failed with status {status:?}"),
                    };
                }

                if resumed || !allow_preview {
                    bail!("unexpected preview response from print dialog");
                }

                let (resume, mut pending) = oneshot::channel::<ResumeRequest>();
                let prepared = PreparedPrint {
                    response,
                    session: Some(PreviewSession { resume }),
                };

                if let Some(completion) = completion.take() {
                    let _ = completion.send(Ok(Outcome::Accepted(prepared)));
                }

                line.clear();
                let resume = tokio::select! {
                    result = &mut pending => match result {
                        Ok(resume) => resume,
                        Err(_) => return Ok(Outcome::Cancelled),
                    },
                    _ = child.wait() => return Ok(Outcome::Cancelled),
                    _ = stdout.read_line(&mut line) => return Ok(Outcome::Cancelled),
                    () = time::sleep(TOKEN_LIFETIME) => return Ok(Outcome::Cancelled),
                };

                request = resume.request;
                document = Some(resume.document);
                completion = Some(resume.completion);
            }

            bail!("print dialog did not submit")
        }
        .await;

        // kill() also waits, so every path releases the process before its document.
        let _ = child.kill().await;
        drop(document);

        if let Some(completion) = completion {
            let _ = completion.send(outcome);
        }
    });

    result.await.context("print dialog supervisor stopped")?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portals::print::tokens::Tokens;
    use tokio::sync::oneshot::error::TryRecvError;

    #[test]
    fn preview_session_follows_its_one_use_app_bound_token() {
        let mut tokens = Tokens::default();
        let (resume, mut pending) = oneshot::channel();
        let prepared = PreparedPrint {
            response: PrintResponse {
                action: PrintAction::Preview,
                ..Default::default()
            },
            session: Some(PreviewSession { resume }),
        };
        let token = tokens.insert("org.example.App", prepared);
        let prepared = tokens.take("org.example.App", token).unwrap();
        assert_eq!(prepared.response.action, PrintAction::Preview);
        assert!(prepared.session.is_some());
        assert!(tokens.take("org.example.App", token).is_none());
        assert!(matches!(pending.try_recv(), Err(TryRecvError::Empty)));
        drop(prepared);
        assert!(matches!(pending.try_recv(), Err(TryRecvError::Closed)));
    }

    #[test]
    fn mismatched_or_dropped_tokens_cancel_the_waiting_session() {
        for mismatched in [false, true] {
            let mut tokens = Tokens::default();
            let (resume, mut pending) = oneshot::channel();
            let token = tokens.insert(
                "org.example.App",
                PreparedPrint {
                    response: PrintResponse {
                        action: PrintAction::Preview,
                        ..Default::default()
                    },
                    session: Some(PreviewSession { resume }),
                },
            );

            if mismatched {
                assert!(tokens.take("org.example.Other", token).is_none());
                assert!(matches!(pending.try_recv(), Err(TryRecvError::Closed)));
            } else {
                drop(tokens);
                assert!(matches!(pending.try_recv(), Err(TryRecvError::Closed)));
            }
        }
    }
}
