use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use iced::{
    Element, Fill, FillPortion, Font, Size, Subscription, Task, keyboard,
    widget::{column, container, image, row, rule, scrollable, text},
    window,
};
use zenzai_core::{
    appearance::UiConfig,
    config,
    ui::{
        dropdown, theme,
        widgets::{self, PANE_PADDING, PANE_SPACING},
    },
};

use crate::{
    cups::{self, Capabilities, Printer},
    paper::Paper,
    preview::{Document, SheetSpec},
    settings::{ColorMode, Duplex, JobSettings, Orientation},
};

use super::{form, output};

const APP_ID: &str = "zenzai-dialog-print";
const WINDOW_SIZE: Size = Size::new(960.0, 620.0);
const DEFAULT_ACCEPT_LABEL: &str = "Print";
const SAVE_ACCEPT_LABEL: &str = "Save";
const CANCEL_LABEL: &str = "Cancel";
const SAVE_AS_PDF_LABEL: &str = "Save as PDF";
const NO_PRINTERS: &str = "No printers found";
const NOT_ACCEPTING: &str = "Printer is not accepting jobs";
const SETTINGS_PADDING: [u16; 2] = [0, 12];
const PERCENT: f64 = 100.0;

// Keep recently viewed sides to avoid re-rendering during navigation.
const PREVIEW_CACHE_SIZE: usize = 32;

#[derive(Clone)]
pub struct Dialog {
    pub title: String,
    pub settings: JobSettings,
    pub document: Option<Arc<Document>>,
    pub has_document: bool,
    pub preferred_printer: Option<String>,
    pub save_as_pdf: bool,
    pub output: PathBuf,
    pub accept_label: Option<String>,
    pub allow_pdf: bool,
    pub has_current_page: bool,
    pub has_selected_pages: bool,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Cancelled,
    Print {
        printer: String,
        settings: JobSettings,
    },
    SavePdf {
        output: PathBuf,
        settings: JobSettings,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Printer(Printer),
    SavePdf,
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Printer(printer) => f.write_str(&printer.description),
            Self::SavePdf => f.write_str(SAVE_AS_PDF_LABEL),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Preview {
    Rendering,
    Ready(image::Handle),
    Failed,
}

#[derive(Debug, Clone)]
pub enum Message {
    Printers(Result<Vec<Printer>, String>),
    Capabilities(String, Result<Capabilities, String>),
    SelectDestination(Destination),
    ToggleMoreSettings,
    Dropdown(dropdown::Action),
    Settings(JobSettings),
    ChooseOutput,
    OutputChosen(Result<Option<PathBuf>, String>),
    OutputTyped(String),
    PreviewSide(usize),
    PreviewRendered(usize, SheetSpec, Preview),
    Accept,
    Cancel,
}

pub struct State {
    pub(super) dialog: Dialog,
    pub(super) more_settings: bool,
    pub(super) menu: Option<dropdown::Menu>,
    pub(super) destinations: Vec<Destination>,
    pub(super) selected: Option<Destination>,
    pub(super) capabilities: Option<Capabilities>,
    error: Option<String>,
    pub(super) preview_side: usize,
    // Accepting while the chooser is open would ignore its pending choice.
    pub(super) picking_output: bool,
    pub(super) manual_output: bool,
    // A side's cached preview is valid only for its stored spec.
    previews: HashMap<usize, (SheetSpec, Preview)>,
    outcome: Arc<Mutex<Outcome>>,
}

impl State {
    pub fn is_pdf(&self) -> bool {
        self.selected == Some(Destination::SavePdf)
    }

    // Existing PDFs are saved verbatim; only printers or the application apply page options.
    pub fn page_options_apply(&self) -> bool {
        !(self.is_pdf() && self.dialog.has_document)
    }

    pub fn papers(&self) -> Vec<Paper> {
        self.capabilities
            .as_ref()
            .map(|caps| caps.papers.clone())
            .filter(|papers| !papers.is_empty())
            .unwrap_or_else(Paper::known)
    }

    pub fn sides(&self) -> Vec<Vec<u32>> {
        let Some(document) = &self.dialog.document else {
            return Vec::new();
        };

        match self.page_options_apply() {
            true => self.dialog.settings.sides(document.page_count()),
            false => (0..document.page_count()).map(|page| vec![page]).collect(),
        }
    }

    pub fn preview(&self, sides: &[Vec<u32>]) -> Option<&Preview> {
        let spec = self.sheet_spec(sides, self.preview_side)?;

        self.previews
            .get(&self.preview_side)
            .filter(|(cached, _)| *cached == spec)
            .map(|(_, preview)| preview)
    }

    fn sheet_spec(&self, sides: &[Vec<u32>], side: usize) -> Option<SheetSpec> {
        let settings = &self.dialog.settings;

        let apply = self.page_options_apply();

        Some(SheetSpec {
            pages: sides.get(side)?.clone(),
            paper_mm: settings
                .paper
                .as_ref()
                .filter(|_| apply)
                .map(|paper| (paper.width_mm, paper.height_mm)),
            landscape: apply && settings.orientation == Orientation::Landscape,
            number_up: if apply {
                settings.number_up as usize
            } else {
                1
            },
            layout: settings.number_up_layout,
            scale: if apply {
                f64::from(settings.scale) / PERCENT
            } else {
                1.0
            },
            // a printer without color support prints grayscale whatever is selected
            grayscale: apply
                && (settings.color == ColorMode::Mono
                    || self.capabilities.as_ref().is_some_and(|caps| !caps.color)),
        })
    }

    fn can_accept(&self) -> bool {
        let valid_destination = match &self.selected {
            Some(Destination::Printer(_)) => self
                .capabilities
                .as_ref()
                .is_some_and(|caps| caps.accepting_jobs),
            Some(Destination::SavePdf) => {
                !self.picking_output && !self.dialog.output.as_os_str().is_empty()
            }
            None => false,
        };

        valid_destination && self.dialog.settings.is_valid()
    }

    pub fn subtitle(&self) -> Option<String> {
        let pages = self.dialog.document.as_ref()?.page_count();

        if !self.page_options_apply() {
            return Some(plural(pages, "page"));
        }

        Some(plural(self.dialog.settings.sheet_count(pages), "sheet"))
    }

    fn finish(&self, outcome: Outcome) -> Task<Message> {
        if let Ok(mut current) = self.outcome.lock() {
            *current = outcome;
        }

        iced::exit()
    }
}

fn plural(count: u32, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    }
}

pub fn run(dialog: Dialog) -> Result<Outcome> {
    let config = config::load_or_default().unwrap_or_else(|err| {
        tracing::warn!("{err:#}");
        UiConfig::default()
    });

    let theme = theme(&config);
    let outcome = Arc::new(Mutex::new(Outcome::Cancelled));

    // iced fonts need a 'static name, the config lives for the whole process anyway
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));

    let boot = {
        let outcome = outcome.clone();
        move || boot(dialog.clone(), outcome.clone())
    };

    let defaults = iced::Settings::default();

    iced::application(boot, update, view)
        .title(|state: &State| state.dialog.title.clone())
        .theme(move |_: &State| theme.clone())
        .subscription(subscription)
        .settings(iced::Settings {
            id: Some(APP_ID.to_string()),
            default_font: font.unwrap_or(defaults.default_font),
            default_text_size: config
                .theme
                .text_size
                .map_or(defaults.default_text_size, Into::into),
            ..defaults
        })
        .window(window::Settings {
            size: WINDOW_SIZE,
            platform_specific: window::settings::PlatformSpecific {
                application_id: APP_ID.to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()?;

    let outcome = outcome
        .lock()
        .map(|outcome| outcome.clone())
        .unwrap_or(Outcome::Cancelled);

    Ok(outcome)
}

fn boot(dialog: Dialog, outcome: Arc<Mutex<Outcome>>) -> (State, Task<Message>) {
    let destinations = dialog
        .allow_pdf
        .then_some(Destination::SavePdf)
        .into_iter()
        .collect();

    let mut state = State {
        dialog,
        more_settings: false,
        menu: None,
        destinations,
        selected: None,
        capabilities: None,
        error: None,
        preview_side: 0,
        picking_output: false,
        manual_output: false,
        previews: HashMap::new(),
        outcome,
    };

    let load_printers = Task::perform(blocking(cups::printers), Message::Printers);
    let render = render_previews(&mut state);

    (state, Task::batch([load_printers, render]))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| err.to_string())
        .and_then(|result| result.map_err(|err| format!("{err:#}")))
}

fn render_previews(state: &mut State) -> Task<Message> {
    let Some(document) = state.dialog.document.clone() else {
        return Task::none();
    };

    let sides = state.sides();
    state.preview_side = state.preview_side.min(sides.len().saturating_sub(1));

    let current = state.preview_side;
    let wanted = [Some(current), Some(current + 1), current.checked_sub(1)];

    let specs: Vec<_> = wanted
        .into_iter()
        .flatten()
        .filter_map(|side| Some((side, state.sheet_spec(&sides, side)?)))
        .filter(|(side, spec)| {
            state
                .previews
                .get(side)
                .is_none_or(|(known, _)| known != spec)
        })
        .collect();

    if state.previews.len() >= PREVIEW_CACHE_SIZE {
        state.previews.retain(|side, _| side.abs_diff(current) <= 1);
    }

    let tasks = specs.into_iter().map(|(side, spec)| {
        state
            .previews
            .insert(side, (spec.clone(), Preview::Rendering));

        let document = document.clone();

        Task::perform(
            async move {
                // a panicking renderer (e.g. a malformed PDF) only costs the preview
                let preview = tokio::task::spawn_blocking({
                    let spec = spec.clone();
                    move || document.render(&spec)
                })
                .await
                .map_or(Preview::Failed, Preview::Ready);

                (spec, preview)
            },
            move |(spec, preview)| Message::PreviewRendered(side, spec, preview),
        )
    });

    Task::batch(tasks)
}

fn select(state: &mut State, destination: Destination) -> Task<Message> {
    state.capabilities = None;
    state.selected = Some(destination.clone());

    let render = render_previews(state);

    let Destination::Printer(printer) = destination else {
        return render;
    };

    let name = printer.name.clone();

    let capabilities = Task::perform(
        blocking(move || cups::capabilities(&printer.name)),
        move |result| Message::Capabilities(name.clone(), result),
    );

    Task::batch([render, capabilities])
}

// options chosen for another printer must not leak into this job
fn apply_capabilities(settings: &mut JobSettings, caps: &Capabilities) {
    let paper_supported = settings
        .paper
        .as_ref()
        .is_some_and(|paper| caps.papers.is_empty() || caps.papers.contains(paper));

    if !paper_supported {
        // prefer the same size under the printer's own name over the printer's default
        settings.paper = settings
            .paper
            .as_ref()
            .and_then(|paper| caps.papers.iter().find(|known| known.same_size(paper)))
            .or(caps.default_paper.as_ref())
            .or(caps.papers.first())
            .cloned();
    }

    if !caps.duplex {
        settings.duplex = Duplex::Off;
    }

    if !caps.color {
        settings.color = ColorMode::Auto;
    }

    if settings
        .media_type
        .as_ref()
        .is_some_and(|value| !caps.media_types.iter().any(|choice| &choice.value == value))
    {
        settings.media_type = None;
    }
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Printers(Ok(printers)) => {
            let wanted = state.dialog.preferred_printer.as_deref();

            let initial = match state.dialog.save_as_pdf && state.dialog.allow_pdf {
                true => Some(Destination::SavePdf),
                false => printers
                    .iter()
                    .find(|printer| Some(printer.name.as_str()) == wanted)
                    .or_else(|| printers.iter().find(|printer| printer.is_default))
                    .or(printers.first())
                    .cloned()
                    .map(Destination::Printer)
                    .or_else(|| state.dialog.allow_pdf.then_some(Destination::SavePdf)),
            };

            if printers.is_empty() {
                state.error = Some(NO_PRINTERS.to_string());
            }

            state.destinations = printers
                .into_iter()
                .map(Destination::Printer)
                .chain(state.dialog.allow_pdf.then_some(Destination::SavePdf))
                .collect();

            initial.map_or_else(Task::none, |initial| select(state, initial))
        }
        Message::Printers(Err(err)) => {
            tracing::error!("{err}");
            state.error = Some(err);

            match state.dialog.allow_pdf {
                true => select(state, Destination::SavePdf),
                false => Task::none(),
            }
        }
        Message::Capabilities(name, result) => {
            let selected =
                matches!(&state.selected, Some(Destination::Printer(p)) if p.name == name);

            if !selected {
                return Task::none();
            }

            match result {
                Ok(caps) => {
                    apply_capabilities(&mut state.dialog.settings, &caps);
                    state.error = (!caps.accepting_jobs).then(|| NOT_ACCEPTING.to_string());
                    state.capabilities = Some(caps);
                }
                Err(err) => {
                    tracing::error!("{err}");
                    state.error = Some(err);
                }
            }

            render_previews(state)
        }
        Message::SelectDestination(destination) => {
            state.menu = None;
            state.error = None;
            select(state, destination)
        }
        Message::ToggleMoreSettings => {
            state.more_settings = !state.more_settings;
            Task::none()
        }
        Message::Dropdown(action) => dropdown::update(&mut state.menu, action),
        Message::Settings(settings) => {
            state.menu = None;
            state.dialog.settings = settings;
            render_previews(state)
        }
        Message::ChooseOutput => {
            state.picking_output = true;

            Task::perform(
                output::pick(state.dialog.output.clone()),
                Message::OutputChosen,
            )
        }
        Message::OutputChosen(result) => {
            state.picking_output = false;

            match result {
                Ok(Some(path)) => state.dialog.output = path,
                Ok(None) => {}
                Err(err) => {
                    tracing::error!("{err}");
                    state.error = Some(err);
                    state.manual_output = true;
                }
            }

            Task::none()
        }
        Message::OutputTyped(path) => {
            state.dialog.output = output::expand(&path);
            Task::none()
        }
        Message::PreviewSide(side) => {
            state.preview_side = side;
            render_previews(state)
        }
        Message::PreviewRendered(side, spec, preview) => {
            // Ignore renders superseded by newer settings.
            if let Some((known, entry)) = state.previews.get_mut(&side)
                && *known == spec
            {
                *entry = preview;
            }

            Task::none()
        }
        Message::Accept => {
            if !state.can_accept() {
                return Task::none();
            }

            let settings = state.dialog.settings.clone();

            let outcome = match &state.selected {
                Some(Destination::Printer(printer)) => Outcome::Print {
                    printer: printer.name.clone(),
                    settings,
                },
                Some(Destination::SavePdf) => Outcome::SavePdf {
                    output: state.dialog.output.clone(),
                    settings,
                },
                None => return Task::none(),
            };

            state.finish(outcome)
        }
        Message::Cancel => state.finish(Outcome::Cancelled),
    }
}

fn subscription(_state: &State) -> Subscription<Message> {
    keyboard::listen().filter_map(|event| match event {
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        } => Some(Message::Cancel),
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            ..
        } => Some(Message::Accept),
        _ => None,
    })
}

fn view(state: &State) -> Element<'_, Message> {
    let accept_label = match (&state.dialog.accept_label, state.is_pdf()) {
        (_, true) => SAVE_ACCEPT_LABEL,
        (Some(label), false) => label.as_str(),
        (None, false) => DEFAULT_ACCEPT_LABEL,
    };

    let content = row![
        container(form::preview(state)).width(FillPortion(1)),
        scrollable(container(form::settings(state)).padding(SETTINGS_PADDING))
            .width(FillPortion(1))
            .height(Fill),
    ]
    .spacing(PANE_SPACING);

    let actions = widgets::action_bar(
        messages(state),
        CANCEL_LABEL,
        Message::Cancel,
        accept_label,
        state.can_accept().then_some(Message::Accept),
    );

    column![
        container(content).padding(PANE_PADDING).height(Fill),
        rule::horizontal(1),
        actions,
    ]
    .into()
}

fn messages(state: &State) -> Option<Element<'_, Message>> {
    let error = state.error.iter().map(|error| (error.as_str(), true));
    let reasons = state
        .capabilities
        .iter()
        .flat_map(|caps| &caps.state_reasons)
        .map(|reason| (reason.label.as_str(), reason.is_error));

    let lines: Vec<Element<'_, Message>> = error
        .chain(reasons)
        .map(|(message, is_error)| {
            let style = match is_error {
                true => text::danger,
                false => text::warning,
            };

            text(message).style(style).into()
        })
        .collect();

    (!lines.is_empty()).then(|| column(lines).into())
}
