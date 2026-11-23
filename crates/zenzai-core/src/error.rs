use std::panic::{self, AssertUnwindSafe};

use crate::{
    appearance::UiConfig,
    config,
    ui::{theme, widgets::PANE_PADDING},
};
use anyhow::Error;
use iced::{
    Element, Fill, Font, Size, Task,
    widget::{button, column, container, text, text_editor},
};
use rust_i18n::t;

const WINDOW_SIZE: Size = Size::new(480.0, 280.0);
const SPACING: u32 = 12;
const TITLE_SIZE: u32 = 20;

struct ErrorDialog {
    title: String,
    details: text_editor::Content,
}

#[derive(Debug, Clone)]
enum Message {
    Details(text_editor::Action),
    Close,
}

/// Show a failure when an Iced event loop is still available; otherwise only log it.
pub fn show_error(title: &str, error: &Error) {
    let config: UiConfig = config::load_or_default().unwrap_or_default();
    let theme = theme(&config);
    let defaults = iced::Settings::default();
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));
    let title = title.to_string();
    let details = format!("{error:#}");

    // Iced panics if the display is unavailable or an event loop already existed.
    // This isolated, best-effort window must not replace the original error.
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        iced::application(
            move || ErrorDialog {
                title: title.clone(),
                details: text_editor::Content::with_text(&details),
            },
            update,
            view,
        )
        .title(|state: &ErrorDialog| state.title.clone())
        .theme(move |_: &ErrorDialog| theme.clone())
        .settings(iced::Settings {
            default_font: font.unwrap_or(defaults.default_font),
            default_text_size: config
                .theme
                .text_size
                .map_or(defaults.default_text_size, Into::into),
            ..defaults
        })
        .window_size(WINDOW_SIZE)
        .run()
    }));

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => eprintln!("could not display error dialog: {err}"),
        Err(_) => eprintln!("could not initialize error dialog"),
    }
}

fn update(state: &mut ErrorDialog, message: Message) -> Task<Message> {
    match message {
        Message::Details(action) if !action.is_edit() => state.details.perform(action),
        Message::Details(_) => {}
        Message::Close => return iced::exit(),
    }

    Task::none()
}

fn view(state: &ErrorDialog) -> Element<'_, Message> {
    column![
        text(&state.title).size(TITLE_SIZE).style(text::danger),
        text_editor(&state.details)
            .on_action(Message::Details)
            .height(Fill),
        container(button(text(t!("common.close"))).on_press(Message::Close)).align_right(Fill),
    ]
    .padding(PANE_PADDING)
    .spacing(SPACING)
    .into()
}
