use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use iced::{
    Alignment, Element, Event, Fill, Font, Size, Subscription, Task, event, keyboard,
    widget::{
        button, column, container, image, operation, row, rule, scrollable, svg, text, text_input,
    },
    window,
};
use zenzai_core::{
    appearance::UiConfig,
    config,
    ui::{
        theme,
        widgets::{self, PANE_PADDING},
    },
};

use crate::{
    LauncherType, Request, Response,
    icon::{self, Icon},
    picker::{self, Selection},
};

const APP_ID: &str = "zenzai-dialog-launcher";
const NAME_ID: &str = "launcher-name";
const ICON_SIZE: f32 = 64.0;
const SPACING: u32 = 12;

struct State {
    request: Request,
    icon: Option<Icon>,
    selected_icon: Option<Vec<u8>>,
    picking: bool,
    error: Option<String>,
    outcome: Arc<Mutex<Option<Response>>>,
}

#[derive(Debug, Clone)]
enum Message {
    Name(String),
    PickIcon,
    IconPicked(Result<Option<Selection>, String>),
    Accept,
    Cancel,
}

pub fn run(mut request: Request) -> Result<Option<Response>> {
    let icon = icon::decode(&request.icon)?;
    request.icon = Vec::new();

    if !request.parent_window.is_empty() || request.modal {
        tracing::warn!("native parent-window attachment and modality are not supported by Iced");
    }

    let config: UiConfig = config::load_or_default().unwrap_or_else(|err| {
        tracing::warn!("{err:#}");
        UiConfig::default()
    });
    let theme = theme(&config);
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));
    let defaults = iced::Settings::default();
    let outcome = Arc::new(Mutex::new(None));
    let boot = {
        let outcome = outcome.clone();
        move || {
            let focus = if request.editable_name {
                operation::focus(NAME_ID)
            } else {
                Task::none()
            };
            (
                State {
                    request: request.clone(),
                    icon: icon.clone(),
                    outcome: outcome.clone(),
                    selected_icon: None,
                    picking: false,
                    error: None,
                },
                focus,
            )
        }
    };

    iced::application(boot, update, view)
        .title("Create launcher")
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
            size: Size::new(480.0, 480.0),
            min_size: Some(Size::new(360.0, 360.0)),
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
        Message::Name(name) if state.request.editable_name => state.request.name = name,
        Message::PickIcon if state.request.editable_icon && !state.picking => {
            state.picking = true;
            state.error = None;
            return Task::perform(
                async { picker::pick().await.map_err(|err| format!("{err:#}")) },
                Message::IconPicked,
            );
        }
        Message::IconPicked(result) => {
            state.picking = false;
            match result {
                Ok(Some(selection)) => {
                    state.icon = Some(selection.preview);
                    state.selected_icon = Some(selection.bytes);
                }
                Ok(None) => {}
                Err(err) => state.error = Some(err),
            }
        }
        Message::Accept if !state.picking && !state.request.name.trim().is_empty() => {
            if let Ok(mut outcome) = state.outcome.lock() {
                *outcome = Some(Response {
                    name: state.request.name.clone(),
                    icon: state.selected_icon.take(),
                });
            }

            return iced::exit();
        }
        Message::Cancel => return iced::exit(),
        _ => {}
    }

    Task::none()
}

fn subscription(_: &State) -> Subscription<Message> {
    event::listen_with(|event, _, _| match event {
        Event::Window(window::Event::CloseRequested) => Some(Message::Cancel),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            ..
        }) => match key {
            keyboard::key::Named::Escape => Some(Message::Cancel),
            keyboard::key::Named::Enter => Some(Message::Accept),
            _ => None,
        },
        _ => None,
    })
}

fn view(state: &State) -> Element<'_, Message> {
    let description = match state.request.launcher_type {
        LauncherType::Application => "Add an application to your app menu.",
        LauncherType::Webapp => "Add a website to your app menu.",
    };
    let mut content = column![
        text("Create launcher").size(24),
        text(description).style(text::secondary),
        text(format!("Requested by: {}", state.request.app_id)).style(text::secondary),
    ]
    .spacing(SPACING);
    let mut preview = row![].spacing(20).align_y(Alignment::Center);
    if let Some(icon) = &state.icon {
        let icon: Element<'_, Message> = match icon {
            Icon::Raster(handle) => image(handle.clone())
                .width(ICON_SIZE)
                .height(ICON_SIZE)
                .into(),
            Icon::Vector(handle) => svg(handle.clone())
                .width(ICON_SIZE)
                .height(ICON_SIZE)
                .into(),
        };
        preview = preview.push(icon);
    }

    if state.request.editable_icon {
        preview = preview.push(
            button(if state.picking {
                "Opening…"
            } else {
                "Change icon…"
            })
            .on_press_maybe((!state.picking).then_some(Message::PickIcon)),
        );
    }

    content = content.push(preview);
    if let Some(error) = &state.error {
        content = content.push(text(error).style(text::danger));
    }

    content = content.push(text("Name").size(14));

    content = if state.request.editable_name {
        content.push(
            text_input("Launcher name", &state.request.name)
                .id(NAME_ID)
                .on_input(Message::Name),
        )
    } else {
        content.push(text(&state.request.name))
    };

    if let Some(target) = &state.request.target
        && matches!(state.request.launcher_type, LauncherType::Webapp)
    {
        content = content.push(
            column![
                text("Website").size(14),
                text(target).style(text::secondary)
            ]
            .spacing(6),
        );
    }

    column![
        container(scrollable(content))
            .padding(PANE_PADDING)
            .height(Fill),
        rule::horizontal(1),
        widgets::action_bar(
            None,
            "Cancel",
            Message::Cancel,
            "Create",
            (!state.picking && !state.request.name.trim().is_empty()).then_some(Message::Accept)
        ),
    ]
    .into()
}
