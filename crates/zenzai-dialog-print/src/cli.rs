use std::path::PathBuf;

use clap::Parser;

use crate::{
    paper::Paper,
    settings::{ColorMode, Duplex, NUMBER_UP_CHOICES, Orientation, PageSelection},
    ui::Dialog,
};

const SAVE_AS_PDF_PRINTER: &str = "pdf";

/// Opens the zenzai print dialog, options pre-fill the dialog.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// PDF document to print, "-" reads from stdin
    pub file: Option<PathBuf>,

    /// Preselect a printer by its CUPS name, "pdf" selects Save as PDF
    #[arg(short = 'P', long)]
    pub printer: Option<String>,

    /// Paper size: a3, a4, a5, a6, b4, b5, letter, legal, tabloid, ... or a PWG media name
    #[arg(short, long, value_parser = parse_paper)]
    pub paper: Option<Paper>,

    #[arg(short, long, value_enum)]
    pub orientation: Option<Orientation>,

    /// Number of copies
    #[arg(short = 'n', long, value_parser = clap::value_parser!(u32).range(1..=999))]
    pub copies: Option<u32>,

    /// Pages to print, 1-based, e.g. "1-3,7"
    #[arg(long)]
    pub pages: Option<String>,

    #[arg(short, long, value_enum)]
    pub duplex: Option<Duplex>,

    #[arg(short, long, value_enum)]
    pub color: Option<ColorMode>,

    /// Pages per sheet
    #[arg(long, value_parser = parse_number_up)]
    pub number_up: Option<u32>,

    /// Title shown in the dialog and used as job name, defaults to the file name
    #[arg(short, long)]
    pub title: Option<String>,

    /// Default output path for Save as PDF
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Used by xdg-desktop-portal-zenzai: reads a JSON request from stdin
    #[arg(long, hide = true, conflicts_with = "file")]
    pub portal: bool,
}

impl Cli {
    pub fn apply(self, dialog: &mut Dialog) {
        let settings = &mut dialog.settings;

        settings.paper = self.paper.or(settings.paper.take());
        settings.orientation = self.orientation.unwrap_or(settings.orientation);
        settings.copies = self.copies.unwrap_or(settings.copies);
        settings.duplex = self.duplex.unwrap_or(settings.duplex);
        settings.color = self.color.unwrap_or(settings.color);
        settings.number_up = self.number_up.unwrap_or(settings.number_up);

        if let Some(pages) = self.pages {
            settings.pages = PageSelection::Ranges;
            settings.ranges = pages;
        }

        match self.printer.as_deref() {
            Some(SAVE_AS_PDF_PRINTER) => dialog.save_as_pdf = true,
            Some(printer) => dialog.preferred_printer = Some(printer.to_string()),
            None => {}
        }

        if let Some(output) = self.output {
            dialog.output = output;
        }
    }
}

fn parse_paper(value: &str) -> Result<Paper, String> {
    Paper::parse(value).ok_or_else(|| format!("unknown paper size {value:?}"))
}

fn parse_number_up(value: &str) -> Result<u32, String> {
    value
        .parse()
        .ok()
        .filter(|n| NUMBER_UP_CHOICES.contains(n))
        .ok_or_else(|| format!("must be one of {NUMBER_UP_CHOICES:?}"))
}
