use std::{
    io::{self, Read, Write},
    process::ExitCode,
};

use anyhow::{Context, Result, ensure};
use clap::Parser;
use rust_i18n::t;
use zenzai_core::ipc::{
    AccessRequest, EXIT_CANCELLED, EXIT_ERROR, EXIT_SUCCESS, MAX_ACCESS_MESSAGE_BYTES,
};

mod ui;

rust_i18n::i18n!("../../locales", fallback = "en");
#[derive(Parser)]
#[command(version, about = "Approve or deny an access request")]
struct Cli {
    /// Open a sample access request
    #[arg(long)]
    demo: bool,
    /// Read a portal request from stdin
    #[arg(long, hide = true, conflicts_with = "demo")]
    portal: bool,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn main() -> ExitCode {
    zenzai_core::i18n::init();

    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let code = match run(&Cli::parse()) {
        Ok(true) => EXIT_SUCCESS,
        Ok(false) => EXIT_CANCELLED,
        Err(err) => {
            eprintln!("zenzai-dialog-access: {err:#}");
            EXIT_ERROR
        }
    };
    ExitCode::from(code as u8)
}

fn run(cli: &Cli) -> Result<bool> {
    let request = if cli.demo {
        AccessRequest {
            app_id: "org.example.Demo".into(),
            parent_window: String::new(),
            title: t!("access.demo.allow-microphone-access").into(),
            subtitle: t!("access.demo.demo-application-wants-to-use-your-microphone").into(),
            body: t!("access.demo.only-accept-if-you-trust-this-application").into(),
            deny_label: Some(t!("common.deny").into()),
            grant_label: Some(t!("common.accept").into()),
            icon: Some("audio-input-microphone".into()),
            image: None,
            modal: false,
            choices: Vec::new(),
        }
    } else {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .take(MAX_ACCESS_MESSAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("could not read access request")?;
        ensure!(
            bytes.len() as u64 <= MAX_ACCESS_MESSAGE_BYTES,
            "access request is too large"
        );
        serde_json::from_slice(&bytes).context("invalid access request")?
    };

    let Some(response) = ui::run(request)? else {
        return Ok(false);
    };

    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(true)
}
