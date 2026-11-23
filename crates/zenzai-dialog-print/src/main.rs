use rust_i18n::t;

use std::{
    fs,
    io::{self, Write},
    panic,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use tempfile::NamedTempFile;
use tracing_subscriber::EnvFilter;
use url::Url;
use zenzai_core::ipc::{
    EXIT_CANCELLED, EXIT_ERROR, EXIT_SUCCESS, PageSetup, PrintAction, PrintRequest, PrintResponse,
    PrintSettings,
};

use cli::Cli;
use preview::Document;
use remember::Remembered;
use settings::{JobSettings, PDF_FORMAT, key};
use ui::{Dialog, Outcome};

mod cli;
mod cups;
mod paper;
mod preview;
mod remember;
mod settings;
mod ui;

rust_i18n::i18n!("../../locales", fallback = "en");

const PDF_MAGIC: &[u8] = b"%PDF";
const PDF_EXTENSION: &str = "pdf";
const STDIN_PATH: &str = "-";

// IPC exit codes are fixed constants in the u8 range.
#[allow(clippy::cast_possible_truncation)]
fn main() -> ExitCode {
    zenzai_core::i18n::init();

    // stdout is reserved for results, log to stderr
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let result = if cli.portal {
        run_portal()
    } else {
        run_cli(cli)
    };

    match result {
        Ok(true) => ExitCode::from(EXIT_SUCCESS as u8),
        Ok(false) => ExitCode::from(EXIT_CANCELLED as u8),
        Err(err) => {
            eprintln!("zenzai-dialog-print: {err:#}");
            zenzai_core::show_error(&t!("print.printing-failed"), &err);
            ExitCode::from(EXIT_ERROR as u8)
        }
    }
}

fn run_cli(cli: Cli) -> Result<bool> {
    let file = cli
        .file
        .as_deref()
        .context("no document given, see --help")?;
    let stdin_copy = (file == Path::new(STDIN_PATH))
        .then(copy_stdin)
        .transpose()?;

    let title = cli.title.clone().or_else(|| {
        file.file_name()
            .filter(|_| stdin_copy.is_none())
            .map(|name| name.to_string_lossy().into_owned())
    });

    let request = PrintRequest {
        title: title.unwrap_or_else(|| t!("print.print-document").into_owned()),
        document: Some(
            stdin_copy
                .as_ref()
                .map_or(file, |copy| copy.path())
                .to_path_buf(),
        ),
        interactive: true,
        ..Default::default()
    };

    let mut dialog = dialog(&request)?;

    if dialog.document.is_none() {
        bail!("{} is not a readable PDF document", file.display());
    }

    cli.apply(&mut dialog);

    let outcome = ui::run(dialog, None)?.outcome;

    let Some(result) = carry_out(&request, &outcome)? else {
        return Ok(false);
    };

    println!("{result}");
    Ok(true)
}

fn run_portal() -> Result<bool> {
    let mut request = read_request()?;

    tracing::debug!(
        document = ?request.document,
        interactive = request.interactive,
        prerendered = request.prerendered,
        "received print request"
    );

    let outcome = if request.interactive {
        let completion = ui::run(dialog(&request)?, Some(request.clone()))?;

        if let Some(updated_request) = completion.request {
            request = updated_request;
        }

        completion.outcome
    } else {
        prepared(&request)?
    };

    if matches!(outcome, Outcome::Cancelled) {
        return Ok(false);
    }

    carry_out(&request, &outcome)?;
    write_response(&request, &outcome, PrintAction::Submit)?;
    Ok(true)
}

fn read_request() -> Result<PrintRequest> {
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("could not read print request")?;

    serde_json::from_str(&line).context("could not parse print request")
}

fn write_response(request: &PrintRequest, outcome: &Outcome, action: PrintAction) -> Result<()> {
    let (settings, printer, output) = match outcome {
        Outcome::Cancelled => bail!("cannot respond to a cancelled print dialog"),
        Outcome::Print { printer, settings } => (settings, Some(printer.as_str()), None),
        Outcome::SavePdf { output, settings } => (settings, None, Some(output.as_path())),
    };

    let response = PrintResponse {
        settings: settings.to_portal(printer, output),
        page_setup: settings.page_setup(&request.page_setup),
        action,
    };

    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn preview_request(request: &PrintRequest, outcome: &Outcome) -> Result<PrintRequest> {
    write_response(request, outcome, PrintAction::Preview)?;
    let request = read_request()?;

    if request.document.is_none() || !request.prerendered {
        bail!("preview request must contain a prerendered document");
    }

    Ok(request)
}

// GTK's locale defaults seed the dialog; remembered choices and explicit settings take precedence.
fn dialog(request: &PrintRequest) -> Result<Dialog> {
    let remembered = Remembered::load();

    let mut settings = JobSettings::default();
    settings.apply_portal(&PrintSettings::new(), &request.page_setup);
    settings.apply_portal(&remembered.settings, &PageSetup::new());
    settings.apply_portal(&request.settings, &PageSetup::new());

    // applications may also send PostScript, which can neither be previewed nor saved as PDF
    let data = request.document.as_deref().map(fs::read).transpose()?;
    let is_pdf = data.as_ref().is_none_or(|data| data.starts_with(PDF_MAGIC));

    let pdf_supported = request
        .supported_output_formats
        .as_ref()
        .is_none_or(|formats| formats.iter().any(|f| f.eq_ignore_ascii_case(PDF_FORMAT)));

    let title = if request.title.is_empty() {
        t!("print.print-document").into_owned()
    } else {
        request.title.clone()
    };

    Ok(Dialog {
        output: default_output(&title),
        title,
        settings,
        allow_pdf: pdf_supported && is_pdf,
        has_document: data.is_some(),
        document: data.filter(|_| is_pdf).and_then(|data| {
            match panic::catch_unwind(|| Document::open(data)) {
                Ok(Ok(document)) => Some(Arc::new(document)),
                Ok(Err(err)) => {
                    tracing::warn!("could not load print preview: {err:#}");
                    None
                }
                Err(_) => {
                    tracing::warn!("print preview parser panicked");
                    None
                }
            }
        }),
        preferred_printer: request
            .settings
            .get(key::PRINTER)
            .cloned()
            .or(remembered.printer),
        save_as_pdf: false,
        accept_label: request.accept_label.clone(),
        has_current_page: request.has_current_page,
        has_selected_pages: request.has_selected_pages,
    })
}

// printing a prepared job must use exactly the stored settings
fn prepared(request: &PrintRequest) -> Result<Outcome> {
    let mut settings = JobSettings::default();
    settings.apply_portal(&request.settings, &request.page_setup);

    let output = request
        .settings
        .get(key::OUTPUT_URI)
        .and_then(|output| Url::parse(output).ok()?.to_file_path().ok());

    match (output, request.settings.get(key::PRINTER)) {
        (Some(output), _) => Ok(Outcome::SavePdf { output, settings }),
        (None, Some(printer)) => Ok(Outcome::Print {
            printer: printer.clone(),
            settings,
        }),
        (None, None) => bail!("no printer selected"),
    }
}

// Returns the CUPS job ID or saved path for CLI scripts.
fn carry_out(request: &PrintRequest, outcome: &Outcome) -> Result<Option<String>> {
    match outcome {
        Outcome::Cancelled => return Ok(None),
        Outcome::Print { printer, settings } => Remembered::save(Some(printer), settings),
        Outcome::SavePdf { settings, .. } => Remembered::save(None, settings),
    }

    // PreparePrint has no document; the application submits it later.
    let Some(document) = request.document.as_deref() else {
        return Ok(None);
    };

    match outcome {
        Outcome::Cancelled => Ok(None),
        Outcome::Print { printer, settings } => {
            let job = cups::print(
                printer,
                &request.title,
                document,
                settings,
                request.prerendered,
            )?;
            Ok(Some(job.to_string()))
        }
        Outcome::SavePdf { output, .. } => {
            save_pdf(document, output)?;
            Ok(Some(output.display().to_string()))
        }
    }
}

fn copy_stdin() -> Result<NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .suffix(&format!(".{PDF_EXTENSION}"))
        .tempfile()?;
    io::copy(&mut io::stdin().lock(), &mut file)?;

    Ok(file)
}

fn save_pdf(document: &Path, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(document, output).with_context(|| format!("could not save {}", output.display()))?;

    Ok(())
}

fn default_output(title: &str) -> PathBuf {
    let name = title.replace('/', "_");
    let name = name.trim_matches('.');
    let name = if name.is_empty() {
        t!("print.print-document")
    } else {
        name.into()
    };

    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_default()
        .join(format!("{name}.{PDF_EXTENSION}"))
}
