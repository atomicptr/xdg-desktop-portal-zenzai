use std::sync::{Arc, Mutex};

use anyhow::Result;
use iced::{
    Alignment, Element, Event, Fill, Font, Size, Subscription, Task, Theme, event, keyboard, mouse,
    widget::{
        checkbox, column, container, image, mouse_area, operation, row, rule, scrollable, svg,
        text, text_input,
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

use crate::apps::App;

const APP_ID: &str = "zenzai-app-select";
const WINDOW_SIZE: Size = Size::new(460.0, 520.0);
const SEARCH_THRESHOLD: usize = 5;
const SEARCH_ID: &str = "app-search";
const LIST_ID: &str = "app-list";
const SPACING: u32 = 12;
const ROW_HEIGHT: f32 = 48.0;
const ICON_SIZE: f32 = 28.0;

type Outcome = Option<(usize, bool)>;

struct State {
    apps: Arc<[App]>,
    suggested: Vec<usize>,
    all: Vec<usize>,
    visible: Vec<usize>,
    title: String,
    configured: bool,
    show_all: bool,
    remember: bool,
    query: String,
    selected: Option<usize>,
    outcome: Arc<Mutex<Outcome>>,
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone)]
enum Message {
    Search(String),
    ShowAll(bool),
    Remember(bool),
    Select(usize),
    Open(usize),
    Navigate(Direction),
    Accept,
    Cancel,
}

impl State {
    fn scope(&self) -> &[usize] {
        if self.show_all {
            &self.all
        } else {
            &self.suggested
        }
    }

    fn search_shown(&self) -> bool {
        self.scope().len() > SEARCH_THRESHOLD
    }

    fn refresh(&mut self) {
        let query = self.query.to_lowercase();
        let matches = |index: &usize| {
            let app = &self.apps[*index];

            query.is_empty()
                || app.label.to_lowercase().contains(&query)
                || app
                    .id
                    .as_ref()
                    .is_some_and(|id| id.to_lowercase().contains(&query))
        };

        self.visible = self.scope().iter().copied().filter(matches).collect();

        self.select(self.selected);
    }

    fn can_remember(&self) -> bool {
        self.selected
            .and_then(|index| self.apps.get(index))
            .is_some_and(|app| app.id.is_some())
    }

    fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|index| self.visible.contains(index));

        if !self.can_remember() {
            self.remember = false;
        }
    }

    fn accept(&self) -> Task<Message> {
        let Some(index) = self.selected.filter(|index| self.visible.contains(index)) else {
            return Task::none();
        };

        self.finish(Some((index, self.remember && self.can_remember())))
    }

    fn finish(&self, result: Outcome) -> Task<Message> {
        if let Ok(mut outcome) = self.outcome.lock() {
            *outcome = result;
        }

        iced::exit()
    }

    fn navigate(&mut self, direction: Direction) -> Task<Message> {
        if self.visible.is_empty() {
            return Task::none();
        }

        let current = self
            .selected
            .and_then(|selected| self.visible.iter().position(|index| *index == selected));
        let last = self.visible.len() - 1;
        let position = match (current, direction) {
            (Some(position), Direction::Up) => position.saturating_sub(1),
            (Some(position), Direction::Down) => (position + 1).min(last),
            (None, Direction::Up) => last,
            (None, Direction::Down) => 0,
        };

        self.select(Some(self.visible[position]));

        // Fixed-height rows keep the selected entry visible at this relative offset.
        operation::snap_to(
            LIST_ID,
            operation::RelativeOffset {
                x: 0.0,
                y: position as f32 / last.max(1) as f32,
            },
        )
    }
}

pub fn run(
    apps: Arc<[App]>,
    suggested: Vec<usize>,
    title: String,
    configured: bool,
) -> Result<Outcome> {
    let config = config::load_or_default().unwrap_or_else(|err| {
        tracing::warn!("{err:#}");
        UiConfig::default()
    });
    let theme = theme(&config);
    let outcome = Arc::new(Mutex::new(None));

    // iced fonts need a static name, and this process owns a single dialog.
    let font = config.theme.font.map(|name| Font::with_name(name.leak()));
    let defaults = iced::Settings::default();
    debug_assert!(suggested.iter().all(|index| *index < apps.len()));
    let all: Vec<_> = (0..apps.len())
        .filter(|index| apps[*index].menu_visible || suggested.contains(index))
        .collect();
    let boot = {
        let outcome = outcome.clone();
        move || {
            let mut state = State {
                apps: apps.clone(),
                suggested: suggested.clone(),
                all: all.clone(),
                visible: Vec::new(),
                title: title.clone(),
                configured,
                show_all: false,
                remember: false,
                query: String::new(),
                selected: None,
                outcome: outcome.clone(),
            };

            state.refresh();

            let focus = if state.search_shown() {
                operation::focus(SEARCH_ID)
            } else {
                Task::none()
            };

            (state, focus)
        }
    };

    iced::application(boot, update, view)
        .title(|state: &State| state.title.clone())
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
            min_size: Some(Size::new(360.0, 360.0)),
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: APP_ID.to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()?;

    Ok(outcome.lock().map(|outcome| *outcome).unwrap_or_default())
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Search(query) => {
            state.query = query;
            state.refresh();

            operation::snap_to(LIST_ID, operation::RelativeOffset::START)
        }
        Message::ShowAll(show_all) => {
            let had_search = state.search_shown();
            state.show_all = show_all;

            if !state.search_shown() {
                state.query.clear();
            }

            state.refresh();

            let focus = if state.search_shown() && !had_search {
                operation::focus(SEARCH_ID)
            } else {
                Task::none()
            };

            Task::batch([
                operation::snap_to(LIST_ID, operation::RelativeOffset::START),
                focus,
            ])
        }
        Message::Remember(remember) => {
            state.remember = remember && state.can_remember();
            Task::none()
        }
        Message::Select(index) => {
            state.select(Some(index));
            Task::none()
        }
        Message::Open(index) => {
            state.select(Some(index));
            state.accept()
        }
        Message::Navigate(direction) => state.navigate(direction),
        Message::Accept => state.accept(),
        Message::Cancel => state.finish(None),
    }
}

fn subscription(_state: &State) -> Subscription<Message> {
    // Escape must also work while the search input captures keyboard events.
    event::listen_with(|event, _, _| match event {
        Event::Window(window::Event::CloseRequested) => Some(Message::Cancel),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            ..
        }) => match key {
            keyboard::key::Named::Escape => Some(Message::Cancel),
            keyboard::key::Named::Enter => Some(Message::Accept),
            keyboard::key::Named::ArrowUp => Some(Message::Navigate(Direction::Up)),
            keyboard::key::Named::ArrowDown => Some(Message::Navigate(Direction::Down)),
            _ => None,
        },
        _ => None,
    })
}

fn view(state: &State) -> Element<'_, Message> {
    let mut content = column![text(&state.title).size(20)].spacing(SPACING);

    if state.search_shown() {
        content = content.push(
            text_input("Search applications…", &state.query)
                .id(SEARCH_ID)
                .on_input(Message::Search),
        );
    }

    let list: Element<'_, Message> = if state.visible.is_empty() {
        container(text("No applications found").style(text::secondary))
            .center(Fill)
            .into()
    } else {
        scrollable(column(
            state.visible.iter().map(|index| app_row(state, *index)),
        ))
        .id(LIST_ID)
        .height(Fill)
        .into()
    };

    content = content
        .push(list)
        .push(
            checkbox(state.show_all)
                .label("Show all applications")
                .on_toggle(Message::ShowAll),
        )
        .push(
            checkbox(state.remember)
                .label("Set as default")
                .on_toggle_maybe(state.can_remember().then_some(Message::Remember)),
        );

    if state.configured {
        content = content.push(
            text("Configuration rules override remembered choices.")
                .size(12)
                .style(text::secondary),
        );
    }

    column![
        container(content).padding(PANE_PADDING).height(Fill),
        rule::horizontal(1),
        widgets::action_bar(
            None,
            "Cancel",
            Message::Cancel,
            "Open",
            state.selected.map(|_| Message::Accept),
        ),
    ]
    .into()
}

fn app_row(state: &State, index: usize) -> Element<'_, Message> {
    let app = &state.apps[index];
    let icon: Element<'_, Message> = match &app.icon {
        Some(path)
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) =>
        {
            svg(svg::Handle::from_path(path))
                .width(ICON_SIZE)
                .height(ICON_SIZE)
                .into()
        }
        Some(path) => image(image::Handle::from_path(path))
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .into(),
        None => container(text(""))
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .into(),
    };
    let selected = state.selected == Some(index);
    let content = container(
        row![
            icon,
            text(&app.label).width(Fill).wrapping(text::Wrapping::None),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .height(ROW_HEIGHT)
    .padding([8, 10])
    .style(move |theme: &Theme| {
        if selected {
            let pair = theme.extended_palette().primary.weak;

            container::Style {
                background: Some(pair.color.into()),
                text_color: Some(pair.text),
                ..Default::default()
            }
        } else {
            container::Style::default()
        }
    });

    mouse_area(content)
        .on_press(Message::Select(index))
        .on_double_click(Message::Open(index))
        .interaction(mouse::Interaction::Pointer)
        .into()
}
