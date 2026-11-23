use std::{
    io::{self, Read, Write},
    process::ExitCode,
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use rust_i18n::t;
use zenzai_core::ipc::{
    EXIT_CANCELLED, EXIT_ERROR, EXIT_SUCCESS, LauncherRequest as Request,
    LauncherResponse as Response, LauncherType, MAX_LAUNCHER_ICON_BYTES as MAX_ICON_BYTES,
    MAX_LAUNCHER_MESSAGE_BYTES as MAX_REQUEST_BYTES,
};

mod icon;
mod picker;
mod ui;

rust_i18n::i18n!("../../locales", fallback = "en");

#[derive(Parser)]
#[command(version, about = "Approve launcher creation")]
struct Cli {
    /// Open the dialog with sample data without installing a launcher
    #[arg(long, value_enum)]
    demo: Option<Demo>,
    /// Read a portal request from stdin
    #[arg(long, hide = true, conflicts_with = "demo")]
    portal: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Demo {
    Application,
    Web,
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
            eprintln!("zenzai-dialog-launcher: {err:#}");
            EXIT_ERROR
        }
    };

    ExitCode::from(code as u8)
}

fn run(cli: &Cli) -> Result<bool> {
    let request = if let Some(demo) = cli.demo {
        demo_request(demo)
    } else {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .take(MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("could not read launcher request")?;
        ensure!(
            bytes.len() as u64 <= MAX_REQUEST_BYTES,
            "launcher request is too large"
        );
        serde_json::from_slice(&bytes).context("invalid launcher request")?
    };
    ensure!(
        request.icon.len() <= MAX_ICON_BYTES,
        "launcher icon is too large"
    );

    let Some(mut response) = ui::run(request)? else {
        return Ok(false);
    };

    if cli.demo.is_some() {
        response.icon = None;
    }

    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(true)
}

fn demo_request(demo: Demo) -> Request {
    const ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <rect width="64" height="64" rx="12" fill="#b4befe"/>
  <text x="17" y="47" font-size="42" fill="#1e1e2e">Z</text>
</svg>"##;

    let (name, launcher_type, target) = match demo {
        Demo::Application => (
            t!("launcher.demo-application"),
            LauncherType::Application,
            None,
        ),
        Demo::Web => (
            t!("launcher.example-website"),
            LauncherType::Webapp,
            Some("https://example.org".into()),
        ),
    };

    Request {
        app_id: "org.example.Demo".into(),
        name: name.into(),
        launcher_type,
        target,
        icon: ICON.to_vec(),
        editable_name: true,
        editable_icon: true,
        parent_window: String::new(),
        modal: false,
    }
}
