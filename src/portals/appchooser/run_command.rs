use std::string::FromUtf8Error;

use tokio::io::AsyncWriteExt;
use zbus::fdo;

use super::config::Command;

#[derive(Debug)]
pub enum RunCommandError {
    IOError(std::io::Error),
    FailedToOpenStdin,
    Utf8Error(FromUtf8Error),
    Other(String),
}

impl From<RunCommandError> for fdo::Error {
    fn from(value: RunCommandError) -> Self {
        match value {
            RunCommandError::IOError(err) => fdo::Error::IOError(format!("IO Error: {:?}", err)),
            RunCommandError::FailedToOpenStdin => {
                fdo::Error::IOError("failed to open stdin".to_string())
            }
            RunCommandError::Utf8Error(err) => {
                fdo::Error::Failed(format!("could not convert to utf-8 {:?}", err))
            }
            RunCommandError::Other(err) => fdo::Error::Failed(err),
        }
    }
}

impl From<std::io::Error> for RunCommandError {
    fn from(value: std::io::Error) -> Self {
        RunCommandError::IOError(value)
    }
}

impl From<FromUtf8Error> for RunCommandError {
    fn from(value: FromUtf8Error) -> Self {
        RunCommandError::Utf8Error(value)
    }
}

pub async fn run_command(cmd: &Command) -> Result<(), RunCommandError> {
    tracing::info!("Run Command: {:?}", cmd);
    let _ = tokio::process::Command::new(&cmd.command)
        .args(&cmd.arguments.clone().unwrap_or_default())
        .spawn()?;
    Ok(())
}

pub async fn run_picker_command(
    cmd: &Command,
    options: &Vec<String>,
) -> Result<String, RunCommandError> {
    let mut c = tokio::process::Command::new(&cmd.command)
        .args(&cmd.arguments.clone().unwrap_or_default())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let stdin = c.stdin.take();

    if stdin.is_none() {
        return Err(RunCommandError::FailedToOpenStdin);
    }

    let mut stdin = stdin.unwrap();

    let options = options.join("\n");
    let input = options.as_bytes();
    stdin.write_all(input).await?;
    stdin.flush().await?;
    drop(stdin);

    let output = c.wait_with_output().await?;

    let stdout = String::from_utf8(output.stdout)?;

    Ok(stdout)
}
