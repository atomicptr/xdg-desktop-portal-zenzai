use std::{
    fmt,
    fs::File,
    io::Read,
    path::Path,
    ptr,
    sync::{Mutex, MutexGuard, PoisonError},
};

use anyhow::{Context, Result, anyhow};
use cups_rs::{
    ConnectionFlags, Destination, HttpConnection, PrintOptions, create_job_with_options,
    get_all_destinations, get_destination,
    job::{FORMAT_PDF, FORMAT_POSTSCRIPT},
};

use crate::{
    paper::Paper,
    settings::{JobSettings, ipp},
};

// CUPS keeps per-thread state but is not safe to use concurrently on shared objects
static CUPS: Mutex<()> = Mutex::new(());

const POSTSCRIPT_MAGIC: &[u8] = b"%!PS";

const VALUE_TWO_SIDED_PREFIX: &str = "two-sided";
const VALUE_COLOR: &str = "color";

const IGNORED_STATE_REASONS: [&str; 2] = ["none", "cups-waiting-for-job-completed"];
const STATE_REASONS: &str = "printer-state-reasons";
const SEVERITY_SUFFIXES: [&str; 3] = ["-error", "-warning", "-report"];
const ERROR_SUFFIX: &str = "-error";
const CONNECT_TIMEOUT_MS: i32 = 5000;
const CUSTOM_RANGE_PREFIXES: [&str; 2] = ["custom_min_", "custom_max_"];

// CUPS maps and localizes standard PWG media types; other keywords are driver-specific.
const STANDARD_MEDIA_TYPES: [&str; 18] = [
    "auto",
    "stationery",
    "stationery-lightweight",
    "stationery-heavyweight",
    "stationery-coated",
    "stationery-inkjet",
    "stationery-recycled",
    "stationery-letterhead",
    "stationery-preprinted",
    "photographic",
    "photographic-glossy",
    "photographic-high-gloss",
    "photographic-semi-gloss",
    "photographic-matte",
    "cardstock",
    "labels",
    "envelope",
    "transparency",
];

// an IPP keyword with the text CUPS shows for it in the user's language
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub struct StateReason {
    pub label: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printer {
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

// These are independent printer capabilities, not mutually exclusive states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub papers: Vec<Paper>,
    pub default_paper: Option<Paper>,
    pub duplex: bool,
    pub color: bool,
    pub quality: bool,
    pub media_types: Vec<Choice>,
    pub state_reasons: Vec<StateReason>,
    pub accepting_jobs: bool,
}

fn lock() -> MutexGuard<'static, ()> {
    CUPS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn printers() -> Result<Vec<Printer>> {
    let _guard = lock();

    let printers = get_all_destinations()
        .context("could not reach CUPS, is cupsd running?")?
        .into_iter()
        .map(|dest| Printer {
            description: dest
                .info()
                .filter(|info| !info.is_empty())
                .cloned()
                .unwrap_or_else(|| dest.full_name()),
            name: dest.full_name(),
            is_default: dest.is_default,
        })
        .collect();

    Ok(printers)
}

pub fn capabilities(name: &str) -> Result<Capabilities> {
    let _guard = lock();

    let dest = get_destination(name)?;
    let info = dest.get_detailed_info(ptr::null_mut())?;

    // cups_rs leaks each raw destination, so fetch it only once per query.
    let raw = dest.as_ptr();

    // localizing needs a real connection, CUPS returns the raw keyword without one
    let connection = dest
        .connect(ConnectionFlags::Scheduler, Some(CONNECT_TIMEOUT_MS), None)
        .inspect_err(|err| tracing::debug!("labels fall back to IPP keywords: {err}"));
    let http = connection
        .as_ref()
        .map_or(ptr::null_mut(), HttpConnection::as_ptr);

    let supported = |option: &str| {
        info.get_supported_values(http, raw, option)
            .unwrap_or_default()
    };

    let label = |option: &str, value: &str| {
        info.localize_value(http, raw, option, value)
            .unwrap_or_else(|_| value.to_string())
    };

    let mut media_types: Vec<_> = supported(ipp::MEDIA_TYPE)
        .into_iter()
        .map(|value| Choice {
            label: label(ipp::MEDIA_TYPE, &value),
            value,
        })
        .collect();

    // standard types first, the stable sort keeps the driver's order for the rest
    media_types.sort_by_key(|choice| {
        STANDARD_MEDIA_TYPES
            .iter()
            .position(|standard| *standard == choice.value)
            .unwrap_or(STANDARD_MEDIA_TYPES.len())
    });

    let paper = |name: &str| {
        if CUSTOM_RANGE_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return None;
        }

        info.get_media_by_name(http, raw, name, 0)
            .ok()
            .and_then(|size| {
                let label = connection
                    .as_ref()
                    .ok()
                    .and_then(|_| info.localize_media(http, raw, 0, &size).ok());

                // CUPS can canonicalize names; jobs must retain the supported keyword.
                Paper::from_cups(name, &size, label)
            })
            .or_else(|| Paper::from_pwg(name))
    };

    // Only media-supported names are valid job options; custom_min_/max_ describe size limits.
    let mut papers: Vec<_> = supported(ipp::MEDIA)
        .iter()
        .filter_map(|name| paper(name))
        .collect();

    // Drivers may localize distinct media variants to the same label.
    let duplicate_labels: Vec<_> = papers
        .iter()
        .map(|paper| {
            papers.iter().any(|other| {
                other.name != paper.name && other.display_name() == paper.display_name()
            })
        })
        .collect();

    for (paper, duplicate) in papers.iter_mut().zip(duplicate_labels) {
        if duplicate {
            paper.distinguish();
        }
    }

    let default_paper = info
        .get_default_value(http, raw, ipp::MEDIA)
        .ok()
        .flatten()
        .and_then(|name| papers.iter().find(|paper| paper.name == name).cloned());

    Ok(Capabilities {
        papers,
        default_paper,
        duplex: supported(ipp::SIDES)
            .iter()
            .any(|value| value.starts_with(VALUE_TWO_SIDED_PREFIX)),
        color: supported(ipp::COLOR_MODE)
            .iter()
            .any(|value| value == VALUE_COLOR),
        quality: supported(ipp::QUALITY).len() > 1,
        media_types,
        state_reasons: dest
            .state_reasons()
            .into_iter()
            .filter(|reason| !IGNORED_STATE_REASONS.contains(&reason.as_str()))
            .map(|reason| {
                // the catalog has the reason without its severity suffix
                let keyword = SEVERITY_SUFFIXES
                    .iter()
                    .find_map(|suffix| reason.strip_suffix(suffix))
                    .unwrap_or(&reason);

                StateReason {
                    label: label(STATE_REASONS, keyword),
                    is_error: reason.ends_with(ERROR_SUFFIX),
                }
            })
            .collect(),
        accepting_jobs: dest.is_accepting_jobs(),
    })
}

pub fn print(
    printer: &str,
    title: &str,
    document: &Path,
    settings: &JobSettings,
    prerendered: bool,
) -> Result<i32> {
    let _guard = lock();

    let dest: Destination = get_destination(printer)?;

    let options = settings
        .ipp_options(prerendered)
        .into_iter()
        .fold(PrintOptions::new(), |options, (key, value)| {
            options.custom_option(key, value)
        });

    let job = create_job_with_options(&dest, title, &options)?;

    let mut magic = [0; POSTSCRIPT_MAGIC.len()];
    let format = match File::open(document)?.read_exact(&mut magic) {
        Ok(()) if magic == POSTSCRIPT_MAGIC => FORMAT_POSTSCRIPT,
        _ => FORMAT_PDF,
    };

    if let Err(err) = job.submit_file(document, format) {
        let _ = job.cancel();
        return Err(anyhow!(err));
    }

    // the document was submitted as the last one, closing only finalizes the job on the server
    if let Err(err) = job.close() {
        tracing::warn!("could not close job {}: {err}", job.id);
    }

    Ok(job.id)
}
