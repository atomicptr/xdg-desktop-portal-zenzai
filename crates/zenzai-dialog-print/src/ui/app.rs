use rust_i18n::t;

use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use anyhow::Result;
use iced::{
    Alignment, Element, Fill, FillPortion, Font, Size, Subscription, Task, keyboard,
    widget::{button, column, container, image, row, rule, scrollable, text},
    window,
};
use tokio::sync::oneshot;
use zenzai_core::{
    appearance::UiConfig,
    config,
    ipc::PrintRequest,
    ui::{
        dropdown, theme,
        widgets::{PANE_PADDING, PANE_SPACING},
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
const COMPACT_WIDTH: f32 = 850.0;
const COMPACT_HEIGHT: f32 = 450.0;
const ACTION_SPACING: u32 = 8;
const PREVIEW_TITLE_SIZE: u32 = 20;
const SETTINGS_PADDING: [u16; 2] = [0, 12];
const PERCENT: f64 = 100.0;

// Keep recently viewed sides to avoid re-rendering during navigation.
const PREVIEW_CACHE_SIZE: usize = 32;

// These are independent capabilities supplied by the printing application.
#[allow(clippy::struct_excessive_bools)]
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

#[derive(Clone)]
pub struct Completion {
    pub outcome: Outcome,
    pub request: Option<PrintRequest>,
}

#[derive(Clone)]
pub struct PreviewDocument {
    request: PrintRequest,
    document: Option<Arc<Document>>,
}

impl fmt::Debug for PreviewDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreviewDocument")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Settings,
    Waiting,
    Preview,
    WithoutPreview,
    Failed,
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
            Self::SavePdf => f.write_str(&t!("print.save-as-pdf")),
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
    Resized(Size),
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
    RequestPreview,
    PreviewDocument(Result<PreviewDocument, String>),
    Accept,
    Cancel,
}

pub struct State {
    pub(super) icons: form::Icons,
    pub(super) dialog: Dialog,
    pub(super) window_size: Size,
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
    outcome: Arc<Mutex<Completion>>,
    portal_request: Option<PrintRequest>,
    phase: Phase,
}

impl State {
    pub(super) fn compact(&self) -> bool {
        self.window_size.width < COMPACT_WIDTH || self.window_size.height < COMPACT_HEIGHT
    }
    pub fn is_pdf(&self) -> bool {
        self.selected == Some(Destination::SavePdf)
    }

    // Existing PDFs are saved verbatim; only printers or the application apply page options.
    pub fn page_options_apply(&self) -> bool {
        self.phase == Phase::Settings && !(self.is_pdf() && self.dialog.has_document)
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

        if self.page_options_apply() {
            self.dialog.settings.sides(document.page_count())
        } else {
            (0..document.page_count()).map(|page| vec![page]).collect()
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

    pub(super) fn can_accept(&self) -> bool {
        if matches!(self.phase, Phase::Waiting | Phase::Failed) {
            return false;
        }

        if self.phase == Phase::Preview
            && !matches!(self.preview(&self.sides()), Some(Preview::Ready(_)))
        {
            return false;
        }

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
            return Some(t!("print.page-count", count = pages).into_owned());
        }

        let sheets = self.dialog.settings.sheet_count(pages);
        Some(t!("print.sheet-count", count = sheets).into_owned())
    }

    pub(super) fn can_preview(&self) -> bool {
        self.phase == Phase::Settings
            && self.portal_request.is_some()
            && !self.dialog.has_document
            && self.dialog.allow_pdf
    }

    fn selection(&self) -> Option<Outcome> {
        let settings = self.dialog.settings.clone();

        match &self.selected {
            Some(Destination::Printer(printer)) => Some(Outcome::Print {
                printer: printer.name.clone(),
                settings,
            }),
            Some(Destination::SavePdf) => Some(Outcome::SavePdf {
                output: self.dialog.output.clone(),
                settings,
            }),
            None => None,
        }
    }

    fn finish(&self, outcome: Outcome) -> Task<Message> {
        if let Ok(mut current) = self.outcome.lock() {
            *current = Completion {
                outcome,
                request: self.portal_request.clone(),
            };
        }

        iced::exit()
    }
}

pub fn run(dialog: Dialog, request: Option<PrintRequest>) -> Result<Completion> {
    let config = config::load_or_default().unwrap_or_else(|err| {
        tracing::warn!("{err:#}");
        UiConfig::default()
    });

    let icons = form::Icons::load(config.icon_theme.as_deref());
    let theme = theme(&config);
    let outcome = Arc::new(Mutex::new(Completion {
        outcome: Outcome::Cancelled,
        request: None,
    }));

    // iced fonts need a 'static name, the config lives for the whole process anyway
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));

    let boot = {
        let outcome = outcome.clone();
        move || {
            boot(
                dialog.clone(),
                outcome.clone(),
                request.clone(),
                icons.clone(),
            )
        }
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

    let outcome = outcome.lock().map_or_else(
        |_| Completion {
            outcome: Outcome::Cancelled,
            request: None,
        },
        |outcome| outcome.clone(),
    );

    Ok(outcome)
}

fn boot(
    dialog: Dialog,
    outcome: Arc<Mutex<Completion>>,
    portal_request: Option<PrintRequest>,
    icons: form::Icons,
) -> (State, Task<Message>) {
    let destinations = dialog
        .allow_pdf
        .then_some(Destination::SavePdf)
        .into_iter()
        .collect();

    let mut state = State {
        icons,
        dialog,
        window_size: WINDOW_SIZE,
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
        portal_request,
        phase: Phase::Settings,
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
    if state.phase != Phase::Settings
        && !matches!(
            message,
            Message::Resized(_)
                | Message::PreviewSide(_)
                | Message::PreviewRendered(..)
                | Message::PreviewDocument(_)
                | Message::Accept
                | Message::Cancel
        )
    {
        return Task::none();
    }

    match message {
        Message::Resized(size) => {
            state.window_size = size;
            Task::none()
        }
        Message::Printers(Ok(printers)) => {
            let wanted = state.dialog.preferred_printer.as_deref();

            let initial = if state.dialog.save_as_pdf && state.dialog.allow_pdf {
                Some(Destination::SavePdf)
            } else {
                printers
                    .iter()
                    .find(|printer| Some(printer.name.as_str()) == wanted)
                    .or_else(|| printers.iter().find(|printer| printer.is_default))
                    .or(printers.first())
                    .cloned()
                    .map(Destination::Printer)
                    .or_else(|| state.dialog.allow_pdf.then_some(Destination::SavePdf))
            };

            if printers.is_empty() {
                state.error = Some(t!("print.no-printers-found").into_owned());
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
            state.error = Some(t!("print.could-not-load-printers", error = err).into_owned());

            if state.dialog.allow_pdf {
                select(state, Destination::SavePdf)
            } else {
                Task::none()
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
                    state.error = (!caps.accepting_jobs)
                        .then(|| t!("print.printer-is-not-accepting-jobs").into_owned());
                    state.capabilities = Some(caps);
                }
                Err(err) => {
                    tracing::error!("{err}");
                    state.error =
                        Some(t!("print.could-not-load-printer-options", error = err).into_owned());
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
                    state.error =
                        Some(t!("print.could-not-choose-output-file", error = err).into_owned());
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
                if state.phase == Phase::Preview && matches!(preview, Preview::Failed) {
                    state.phase = Phase::WithoutPreview;
                }

                *entry = preview;
            }

            Task::none()
        }
        Message::RequestPreview => {
            if !state.can_preview() || !state.can_accept() {
                return Task::none();
            }

            let Some(request) = state.portal_request.clone() else {
                return Task::none();
            };
            let Some(outcome) = state.selection() else {
                return Task::none();
            };

            state.phase = Phase::Waiting;
            state.menu = None;

            Task::perform(preview_document(request, outcome), Message::PreviewDocument)
        }
        Message::PreviewDocument(result) => match result {
            Ok(preview) => {
                state.portal_request = Some(preview.request);
                state.phase = if preview.document.is_some() {
                    Phase::Preview
                } else {
                    Phase::WithoutPreview
                };
                state.dialog.document = preview.document;
                state.dialog.has_document = true;
                state.preview_side = 0;
                state.previews.clear();
                render_previews(state)
            }
            Err(err) => {
                tracing::warn!("print preview failed: {err}");
                state.error = Some(t!("print.could-not-prepare-preview", error = err).into_owned());
                state.phase = Phase::Failed;
                Task::none()
            }
        },
        Message::Accept => {
            if !state.can_accept() {
                return Task::none();
            }

            state
                .selection()
                .map_or_else(Task::none, |outcome| state.finish(outcome))
        }
        Message::Cancel => state.finish(Outcome::Cancelled),
    }
}

async fn preview_document(
    request: PrintRequest,
    outcome: Outcome,
) -> Result<PreviewDocument, String> {
    let (send, receive) = oneshot::channel();

    // A blocked stdin reader must not prevent Iced's Tokio runtime from shutting down.
    thread::Builder::new()
        .name("print-preview".to_string())
        .spawn(move || {
            let result: Result<PreviewDocument> = (|| {
                let request = crate::preview_request(&request, &outcome)?;
                let dialog = crate::dialog(&request)?;
                let document = dialog.document.filter(|document| document.page_count() > 0);

                Ok(PreviewDocument { request, document })
            })();
            let _ = send.send(result.map_err(|err| format!("{err:#}")));
        })
        .map_err(|err| err.to_string())?;

    receive.await.map_err(|err| err.to_string())?
}

fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch([
        window::resize_events().map(|(_, size)| Message::Resized(size)),
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
        }),
    ])
}

fn view(state: &State) -> Element<'_, Message> {
    let accept_label = match (state.phase, &state.dialog.accept_label, state.is_pdf()) {
        (_, _, true) => t!("common.save"),
        (Phase::Settings, Some(label), false) => label.as_str().into(),
        _ => t!("print.print"),
    };

    if state.phase != Phase::Settings {
        let preview: Element<'_, Message> = if state.phase == Phase::Waiting {
            container(text(t!("print.preparing-preview")))
                .center(Fill)
                .into()
        } else if state.phase == Phase::WithoutPreview {
            container(text(t!("print.preview-unavailable")))
                .center(Fill)
                .into()
        } else {
            form::preview(state)
        };

        return column![
            container(
                column![text(&state.dialog.title).size(PREVIEW_TITLE_SIZE), preview,]
                    .spacing(PANE_SPACING)
            )
            .padding(PANE_PADDING)
            .height(Fill),
            rule::horizontal(1),
            actions(state, accept_label),
        ]
        .into();
    }

    let settings = scrollable(
        container(form::settings(state))
            .padding(SETTINGS_PADDING)
            .width(Fill),
    )
    .direction(scrollable::Direction::Both {
        vertical: scrollable::Scrollbar::default(),
        horizontal: scrollable::Scrollbar::default(),
    })
    .width(Fill)
    .height(Fill);

    let content: Element<'_, Message> = if state.compact() {
        settings.into()
    } else {
        row![
            container(form::preview(state)).width(FillPortion(1)),
            container(settings).width(FillPortion(1)),
        ]
        .spacing(PANE_SPACING)
        .into()
    };

    let actions = actions(state, accept_label);

    column![
        container(content).padding(PANE_PADDING).height(Fill),
        rule::horizontal(1),
        actions,
    ]
    .into()
}

fn actions<'a>(
    state: &'a State,
    accept_label: impl Into<text::Fragment<'a>>,
) -> Element<'a, Message> {
    let buttons = row![
        button(text(t!("common.cancel")))
            .style(button::secondary)
            .on_press(Message::Cancel),
        button(text(accept_label.into()))
            .style(button::primary)
            .on_press_maybe(state.can_accept().then_some(Message::Accept)),
    ]
    .spacing(ACTION_SPACING);

    row![container(messages(state)).width(Fill), buttons]
        .spacing(PANE_SPACING)
        .align_y(Alignment::Center)
        .padding(PANE_PADDING)
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
            let style = if is_error {
                text::danger
            } else {
                text::warning
            };

            text(message).style(style).into()
        })
        .collect();

    (!lines.is_empty()).then(|| column(lines).into())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::settings::PageSelection;

    use iced::{
        Pixels, Renderer,
        advanced::{layout::Limits, widget::Tree},
    };

    use super::*;

    #[test]
    fn print_form_keeps_usable_bounds_when_scrolling() {
        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        let (mut state, _) = boot(
            Dialog {
                title: "Print document".to_string(),
                settings: JobSettings::default(),
                document: None,
                has_document: true,
                preferred_printer: None,
                save_as_pdf: false,
                output: PathBuf::from("document.pdf"),
                accept_label: None,
                allow_pdf: true,
                has_current_page: false,
                has_selected_pages: false,
            },
            Arc::new(Mutex::new(Completion {
                outcome: Outcome::Cancelled,
                request: None,
            })),
            None,
            form::Icons::default(),
        );

        for size in [
            WINDOW_SIZE,
            Size::new(640.0, 480.0),
            Size::new(480.0, 320.0),
        ] {
            for expanded in [false, true] {
                for destination in [None, Some(Destination::SavePdf)] {
                    state.window_size = size;
                    state.more_settings = expanded;
                    state.selected = destination;

                    let mut element = view(&state);
                    let mut tree = Tree::new(element.as_widget());
                    let layout = element.as_widget_mut().layout(
                        &mut tree,
                        &renderer,
                        &Limits::new(Size::ZERO, size),
                    );
                    let content = &layout.children()[0].children()[0];
                    let scroll = if state.compact() {
                        content
                    } else {
                        &content.children()[1].children()[0]
                    };
                    let form = &scroll.children()[0].children()[0];

                    assert!(
                        form.size().width >= 240.0 && form.size().width.is_finite(),
                        "form collapsed at {size:?}, expanded={expanded}, destination={:?}: {:?}",
                        state.selected,
                        form.size(),
                    );
                }
            }
        }
    }

    #[test]
    fn deferred_preview_freezes_settings_and_requires_explicit_submission() {
        let completion = Arc::new(Mutex::new(Completion {
            outcome: Outcome::Cancelled,
            request: None,
        }));
        let (mut state, _) = boot(
            Dialog {
                title: "Print document".to_string(),
                settings: JobSettings {
                    pages: PageSelection::Ranges,
                    ranges: "2-3".to_string(),
                    scale: 50,
                    reverse: true,
                    ..Default::default()
                },
                document: None,
                has_document: false,
                preferred_printer: None,
                save_as_pdf: false,
                output: PathBuf::from("document.pdf"),
                accept_label: None,
                allow_pdf: true,
                has_current_page: false,
                has_selected_pages: false,
            },
            Arc::clone(&completion),
            Some(PrintRequest::default()),
            form::Icons::default(),
        );
        state.selected = Some(Destination::Printer(Printer {
            name: "printer".to_string(),
            description: "Printer".to_string(),
            is_default: true,
        }));
        state.capabilities = Some(Capabilities {
            accepting_jobs: true,
            ..Default::default()
        });
        assert!(state.can_preview());

        let _ = update(&mut state, Message::RequestPreview);
        assert_eq!(state.phase, Phase::Waiting);
        assert!(!state.can_accept());
        let _ = update(&mut state, Message::Settings(JobSettings::default()));
        assert_eq!(state.dialog.settings.scale, 50);
        let _ = update(&mut state, Message::Accept);
        assert!(matches!(
            completion.lock().unwrap().outcome,
            Outcome::Cancelled
        ));

        let pdf = concat!(
            "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>\nendobj\n",
            "xref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n",
            "0000000058 00000 n \n0000000115 00000 n \n",
            "trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n186\n%%EOF\n",
        );
        let _ = update(
            &mut state,
            Message::PreviewDocument(Ok(PreviewDocument {
                request: PrintRequest {
                    document: Some(PathBuf::from("rendered.pdf")),
                    prerendered: true,
                    ..Default::default()
                },
                document: Some(Arc::new(Document::open(pdf.as_bytes().to_vec()).unwrap())),
            })),
        );
        assert_eq!(state.phase, Phase::Preview);
        assert!(!state.can_preview());
        assert!(!state.can_accept());
        let sides = state.sides();
        assert_eq!(sides, [vec![0]]);
        let spec = state.sheet_spec(&sides, 0).unwrap();
        assert_eq!(spec.scale, 1.0);
        assert_eq!(spec.paper_mm, None);
        let handle = state.dialog.document.as_ref().unwrap().render(&spec);
        let _ = update(
            &mut state,
            Message::PreviewRendered(0, spec.clone(), Preview::Ready(handle)),
        );
        assert!(state.can_accept());

        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        for size in [WINDOW_SIZE, Size::new(480.0, 320.0)] {
            state.window_size = size;
            let mut element = view(&state);
            let mut tree = Tree::new(element.as_widget());
            let layout = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &Limits::new(Size::ZERO, size),
            );
            let preview = &layout.children()[0].children()[0].children()[1];
            assert!(
                preview.size().height > 32.0,
                "preview collapsed at {size:?}"
            );
        }

        let _ = update(
            &mut state,
            Message::PreviewRendered(0, spec, Preview::Failed),
        );
        assert_eq!(state.phase, Phase::WithoutPreview);
        assert!(state.can_accept());
        assert!(matches!(
            completion.lock().unwrap().outcome,
            Outcome::Cancelled
        ));

        let _ = update(&mut state, Message::Accept);
        let completed = completion.lock().unwrap();
        assert!(
            matches!(&completed.outcome, Outcome::Print { printer, settings }
            if printer == "printer" && settings.scale == 50)
        );
        assert_eq!(
            completed.request.as_ref().unwrap().document.as_deref(),
            Some(Path::new("rendered.pdf"))
        );
        drop(completed);

        for destination in [state.selected.clone().unwrap(), Destination::SavePdf] {
            state.phase = Phase::Waiting;
            state.selected = Some(destination);
            completion.lock().unwrap().outcome = Outcome::Cancelled;
            let _ = update(
                &mut state,
                Message::PreviewDocument(Ok(PreviewDocument {
                    request: PrintRequest {
                        document: Some(PathBuf::from("unpreviewable.pdf")),
                        prerendered: true,
                        ..Default::default()
                    },
                    document: None,
                })),
            );
            assert_eq!(state.phase, Phase::WithoutPreview);
            assert!(state.can_accept());
            assert!(!state.page_options_apply());
            let _ = update(&mut state, Message::Settings(JobSettings::default()));
            assert_eq!(state.dialog.settings.scale, 50);
            assert!(matches!(
                completion.lock().unwrap().outcome,
                Outcome::Cancelled
            ));

            let _ = update(&mut state, Message::Accept);
            let completed = completion.lock().unwrap();
            assert!(matches!(&completed.outcome,
                Outcome::Print { settings, .. } | Outcome::SavePdf { settings, .. }
                if settings.scale == 50));
            assert_eq!(
                completed.request.as_ref().unwrap().document.as_deref(),
                Some(Path::new("unpreviewable.pdf"))
            );
        }

        completion.lock().unwrap().outcome = Outcome::Cancelled;
        state.phase = Phase::Waiting;
        let _ = update(
            &mut state,
            Message::PreviewDocument(Err("render failed".to_string())),
        );
        assert_eq!(state.phase, Phase::Failed);
        assert!(!state.can_accept());
        let _ = update(&mut state, Message::Accept);
        assert!(matches!(
            completion.lock().unwrap().outcome,
            Outcome::Cancelled
        ));
        let _ = update(&mut state, Message::Cancel);
        assert!(matches!(
            completion.lock().unwrap().outcome,
            Outcome::Cancelled
        ));
    }
}
