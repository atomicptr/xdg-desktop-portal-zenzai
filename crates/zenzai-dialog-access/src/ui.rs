use std::{
    fs::File,
    io::{Cursor, Read},
    str,
    sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};
use iced::{
    Element, Event, Fill, Font, Size, Subscription, Task, Theme, event, keyboard,
    widget::{
        checkbox, column, container, image as raster, radio, row, rule, scrollable, svg, text,
    },
    window,
};
use image::{ImageFormat, ImageReader, Limits};
use rust_i18n::t;
use zenzai_core::{
    appearance::UiConfig,
    config,
    ipc::{AccessRequest, AccessResponse},
    ui::{
        icons, theme,
        widgets::{self, PANE_PADDING},
    },
};

const APP_ID: &str = "zenzai-dialog-access";
const SPACING: u32 = 12;
const ICON_SIZE: u16 = 48;
const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ICON_DIMENSION: u32 = 1024;

#[derive(Clone)]
enum Icon {
    Svg(svg::Handle, bool),
    Png(raster::Handle),
}

fn load_icon(name: &str, theme: Option<&str>) -> Option<Icon> {
    let path = icons::lookup(name, ICON_SIZE, theme)?;
    if !path.metadata().ok()?.is_file() {
        return None;
    }

    let mut bytes = Vec::new();
    File::open(&path)
        .ok()?
        .take(MAX_ICON_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return None;
    }

    match path.extension()?.to_str()? {
        "svg" => {
            let symbolic = name.ends_with("-symbolic")
                || str::from_utf8(&bytes).is_ok_and(|svg| svg.contains("currentColor"));
            Some(Icon::Svg(svg::Handle::from_memory(bytes), symbolic))
        }
        "png" => {
            let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
            let mut limits = Limits::default();
            limits.max_image_width = Some(MAX_ICON_DIMENSION);
            limits.max_image_height = Some(MAX_ICON_DIMENSION);
            limits.max_alloc = Some(MAX_ICON_BYTES);
            reader.limits(limits);
            let pixels = reader.decode().ok()?.into_rgba8();
            Some(Icon::Png(raster::Handle::from_rgba(
                pixels.width(),
                pixels.height(),
                pixels.into_raw(),
            )))
        }
        _ => None,
    }
}

struct State {
    request: AccessRequest,
    outcome: Arc<Mutex<Option<AccessResponse>>>,
    icon: Option<Icon>,
}

#[derive(Debug, Clone)]
enum Message {
    Choice(usize, String),
    Accept,
    Deny,
}

pub fn run(request: AccessRequest) -> Result<Option<AccessResponse>> {
    if !request.parent_window.is_empty() || request.modal {
        tracing::warn!("native parent-window attachment and modality are not supported by Iced");
    }

    let config: UiConfig = config::load_or_default().unwrap_or_else(|err| {
        tracing::warn!("{err:#}");
        UiConfig::default()
    });
    let icon = request
        .icon
        .as_deref()
        .and_then(|name| load_icon(name, config.icon_theme.as_deref()))
        .or_else(|| load_icon("dialog-question", config.icon_theme.as_deref()));
    let theme = theme(&config);
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));
    let defaults = iced::Settings::default();
    let outcome = Arc::new(Mutex::new(None));
    let boot = {
        let outcome = Arc::clone(&outcome);
        move || {
            (
                State {
                    request: request.clone(),
                    outcome: Arc::clone(&outcome),
                    icon: icon.clone(),
                },
                Task::none(),
            )
        }
    };

    iced::application(boot, update, view)
        .title(|state: &State| state.request.title.clone())
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
            size: Size::new(480.0, 300.0),
            min_size: Some(Size::new(360.0, 240.0)),
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: APP_ID.to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()?;

    outcome
        .lock()
        .map(|mut value| value.take())
        .map_err(|_| anyhow!("dialog result lock poisoned"))
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Choice(index, selected) => state.request.choices[index].selected = selected,
        Message::Accept => {
            if let Ok(mut outcome) = state.outcome.lock() {
                *outcome = Some(AccessResponse {
                    choices: state
                        .request
                        .choices
                        .iter()
                        .map(|choice| (choice.id.clone(), choice.selected.clone()))
                        .collect(),
                });
            }

            return iced::exit();
        }
        Message::Deny => return iced::exit(),
    }

    Task::none()
}

fn subscription(_: &State) -> Subscription<Message> {
    event::listen_with(|event, _, _| match event {
        Event::Window(window::Event::CloseRequested) => Some(Message::Deny),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            ..
        }) => match key {
            keyboard::key::Named::Escape => Some(Message::Deny),
            keyboard::key::Named::Enter => Some(Message::Accept),
            _ => None,
        },
        _ => None,
    })
}

fn view(state: &State) -> Element<'_, Message> {
    let request = &state.request;
    let mut content = column![text(&request.title).size(24)].spacing(SPACING);
    for value in [&request.subtitle, &request.body] {
        if !value.is_empty() {
            content = content.push(text(value));
        }
    }

    for (index, choice) in request.choices.iter().enumerate() {
        if choice.options.is_empty() {
            content = content.push(
                checkbox(choice.selected == "true")
                    .label(&choice.label)
                    .on_toggle(move |checked| Message::Choice(index, checked.to_string())),
            );
        } else {
            let mut options = column![text(&choice.label)].spacing(SPACING);
            let selected = choice
                .options
                .iter()
                .position(|(value, _)| value == &choice.selected);
            for (option_index, (id, label)) in choice.options.iter().enumerate() {
                options = options.push(radio(label, option_index, selected, move |_| {
                    Message::Choice(index, id.clone())
                }));
            }

            content = content.push(options);
        }
    }

    column![
        container(scrollable(
            row![
                match &state.icon {
                    Some(Icon::Svg(handle, symbolic)) => svg(handle.clone())
                        .width(u32::from(ICON_SIZE))
                        .height(u32::from(ICON_SIZE))
                        .style(move |theme: &Theme, _| svg::Style {
                            color: symbolic.then_some(theme.palette().text),
                        })
                        .into(),
                    Some(Icon::Png(handle)) => raster(handle.clone())
                        .width(u32::from(ICON_SIZE))
                        .height(u32::from(ICON_SIZE))
                        .into(),
                    None => Element::from(container(text(""))),
                },
                content
            ]
            .spacing(SPACING)
        ))
        .padding(PANE_PADDING)
        .height(Fill),
        rule::horizontal(1),
        widgets::action_bar(
            None,
            request
                .deny_label
                .as_deref()
                .map_or_else(|| t!("common.deny"), Into::into),
            Message::Deny,
            request
                .grant_label
                .as_deref()
                .map_or_else(|| t!("common.accept"), Into::into),
            Some(Message::Accept)
        ),
    ]
    .into()
}
