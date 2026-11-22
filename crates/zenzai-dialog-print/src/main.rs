use std::{
    fs, io,
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
    EXIT_CANCELLED, EXIT_ERROR, EXIT_SUCCESS, PageSetup, PrintRequest, PrintResponse, PrintSettings,
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

const PDF_MAGIC: &[u8] = b"%PDF";
const PDF_EXTENSION: &str = "pdf";
const STDIN_PATH: &str = "-";
const DEFAULT_TITLE: &str = "Print Document";

fn main() -> ExitCode {
    // stdout is reserved for results, log to stderr
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let result = match cli.portal {
        true => run_portal(),
        false => run_cli(cli),
    };

    match result {
        Ok(true) => ExitCode::from(EXIT_SUCCESS as u8),
        Ok(false) => ExitCode::from(EXIT_CANCELLED as u8),
        Err(err) => {
            eprintln!("zenzai-dialog-print: {err:#}");
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
        title: title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
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

    let outcome = ui::run(dialog)?;

    let Some(result) = carry_out(&request, &outcome)? else {
        return Ok(false);
    };

    println!("{result}");
    Ok(true)
}

fn run_portal() -> Result<bool> {
    let request: PrintRequest =
        serde_json::from_reader(io::stdin().lock()).context("could not parse print request")?;

    let outcome = match request.interactive {
        true => ui::run(dialog(&request)?)?,
        false => prepared(&request)?,
    };

    carry_out(&request, &outcome)?;

    let (settings, printer, output) = match &outcome {
        Outcome::Cancelled => return Ok(false),
        Outcome::Print { printer, settings } => (settings, Some(printer.as_str()), None),
        Outcome::SavePdf { output, settings } => (settings, None, Some(output.as_path())),
    };

    let response = PrintResponse {
        settings: settings.to_portal(printer, output),
        page_setup: settings.page_setup(&request.page_setup),
    };

    serde_json::to_writer(io::stdout().lock(), &response)?;
    Ok(true)
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

    let title = match request.title.is_empty() {
        true => DEFAULT_TITLE.to_string(),
        false => request.title.clone(),
    };

    Ok(Dialog {
        output: default_output(&title),
        title,
        settings,
        allow_pdf: pdf_supported && is_pdf,
        has_document: data.is_some(),
        document: data
            .filter(|_| is_pdf)
            .and_then(|data| Document::open(data).ok())
            .map(Arc::new),
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
    let name = match name.is_empty() {
        true => DEFAULT_TITLE,
        false => name,
    };

    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_default()
        .join(format!("{name}.{PDF_EXTENSION}"))
}
