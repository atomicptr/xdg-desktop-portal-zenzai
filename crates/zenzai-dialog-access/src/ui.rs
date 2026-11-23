use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use iced::{
    Element, Event, Fill, Font, Size, Subscription, Task, event, keyboard,
    widget::{checkbox, column, container, radio, row, rule, scrollable, text},
    window,
};
use rust_i18n::t;
use zenzai_core::{
    appearance::UiConfig,
    config,
    ipc::{AccessRequest, AccessResponse},
    ui::{
        icons::{self, Icon},
        theme,
        widgets::{self, PANE_PADDING},
    },
};

const APP_ID: &str = "zenzai-dialog-access";
const SPACING: u32 = 12;
const ICON_SIZE: u16 = 48;

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
        .image
        .as_deref()
        .and_then(|path| {
            icons::load_image(path).or_else(|| {
                tracing::warn!("could not load request image; using icon instead");
                None
            })
        })
        .or_else(|| {
            request
                .icon
                .as_deref()
                .and_then(|name| icons::load(name, ICON_SIZE, config.icon_theme.as_deref()))
        })
        .or_else(|| icons::load("dialog-question", ICON_SIZE, config.icon_theme.as_deref()));
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
                    Some(icon) => icon.view(u32::from(ICON_SIZE)),
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
