use iced::{
    Background, Border, Element, Event, Fill, Point, Rectangle, Renderer, Size, Task, Theme,
    Vector,
    advanced::{
        Clipboard, Layout, Shell,
        layout::{Limits, Node},
        mouse, overlay, renderer,
        text::{self as core_text, Paragraph as _, Renderer as _},
        widget::{Operation, Tree, Widget},
    },
    alignment::Vertical,
    keyboard::{self, Key, key::Named},
    widget::{
        Column, Id, button, container, operation, row, scrollable, scrollable::RelativeOffset,
        text, text_input,
    },
};

// smaller lists are quicker to scan than to search
const SEARCH_THRESHOLD: usize = 5;
const SEARCH_PLACEHOLDER: &str = "Search";
const SEARCH_ID: Id = Id::new("zenzai-dropdown-search");
const LIST_ID: Id = Id::new("zenzai-dropdown-list");
const ARROW: &str = "▾";
const MENU_MAX_HEIGHT: f32 = 320.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: u16 = 4;
const MENU_RADIUS: f32 = 6.0;
const ANCHOR_RADIUS: f32 = 4.0;
const ITEM_PADDING: [u16; 2] = [6, 10];
const BORDER_WIDTH: f32 = 1.0;
const SCROLLBAR_WIDTH: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Search {
    // offered once the list is long enough to be tedious to scan
    Auto,
    Off,
}

// the open menu, at most one exists at a time
#[derive(Debug, Clone)]
pub struct Menu {
    id: &'static str,
    query: String,
    highlighted: usize,
}

#[derive(Debug, Clone)]
pub enum Action {
    // the offsets scroll the highlighted entry into view
    Open(&'static str, usize, f32),
    Close,
    Search(String),
    Highlight(usize, f32),
}

pub fn update<Message: Send + 'static>(menu: &mut Option<Menu>, action: Action) -> Task<Message> {
    match action {
        Action::Open(id, highlighted, offset) => {
            *menu = Some(Menu {
                id,
                query: String::new(),
                highlighted,
            });

            Task::batch([operation::focus(SEARCH_ID), scroll_to(offset)])
        }
        Action::Close => {
            *menu = None;
            Task::none()
        }
        Action::Search(query) => {
            if let Some(menu) = menu {
                menu.query = query;
                menu.highlighted = 0;
            }

            scroll_to(0.0)
        }
        Action::Highlight(highlighted, offset) => {
            if let Some(menu) = menu {
                menu.highlighted = highlighted;
            }

            scroll_to(offset)
        }
    }
}

fn scroll_to<Message: Send + 'static>(offset: f32) -> Task<Message> {
    operation::snap_to(LIST_ID, RelativeOffset { x: 0.0, y: offset })
}

// scrolling proportionally to the index always keeps the entry inside the viewport
fn offset(index: usize, count: usize) -> f32 {
    match count {
        0 | 1 => 0.0,
        _ => index as f32 / (count - 1) as f32,
    }
}

pub fn dropdown<'a, T, Message>(
    id: &'static str,
    options: &[T],
    selected: Option<&T>,
    menu: Option<&'a Menu>,
    search: Search,
    on_action: fn(Action) -> Message,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Clone + PartialEq + ToString,
    Message: Clone + 'a,
{
    let menu = menu.filter(|menu| menu.id == id);
    let is_open = menu.is_some();

    let toggle = match menu {
        Some(_) => Action::Close,
        None => {
            let index = selected
                .and_then(|selected| options.iter().position(|option| option == selected))
                .unwrap_or_default();

            Action::Open(id, index, offset(index, options.len()))
        }
    };

    let label = selected.map(T::to_string).unwrap_or_default();

    let anchor = button(row![
        text(label).wrapping(core_text::Wrapping::None).width(Fill),
        text(ARROW),
    ])
    .width(Fill)
    .clip(true)
    .style(move |theme, status| anchor_style(theme, status, is_open))
    .on_press(on_action(toggle));

    Dropdown {
        anchor: anchor.into(),
        open: menu.map(|menu| open(menu, options, search, on_action, on_select)),
    }
    .into()
}

fn open<'a, T, Message>(
    menu: &'a Menu,
    options: &[T],
    search: Search,
    on_action: fn(Action) -> Message,
    on_select: impl Fn(T) -> Message,
) -> Open<'a, Message>
where
    T: Clone + ToString,
    Message: Clone + 'a,
{
    let labels: Vec<String> = options.iter().map(T::to_string).collect();
    let query = menu.query.to_lowercase();

    let matches: Vec<_> = options
        .iter()
        .zip(&labels)
        .filter(|(_, label)| label.to_lowercase().contains(&query))
        .collect();

    let highlighted = menu.highlighted.min(matches.len().saturating_sub(1));
    let highlight =
        |index: usize| on_action(Action::Highlight(index, offset(index, matches.len())));

    let keys = Keys {
        dismiss: on_action(Action::Close),
        up: highlighted.checked_sub(1).map(highlight),
        down: (highlighted + 1 < matches.len()).then(|| highlight(highlighted + 1)),
        submit: matches
            .get(highlighted)
            .map(|(option, _)| on_select((*option).clone())),
    };

    let items = matches.iter().enumerate().map(|(index, (option, label))| {
        let is_highlighted = index == highlighted;

        button(text(label.to_string()))
            .width(Fill)
            .padding(ITEM_PADDING)
            .style(move |theme, status| item_style(theme, status, is_highlighted))
            .on_press(on_select((*option).clone()))
            .into()
    });

    let search = (search == Search::Auto && options.len() > SEARCH_THRESHOLD).then(|| {
        text_input(SEARCH_PLACEHOLDER, &menu.query)
            .id(SEARCH_ID)
            .on_input(move |query| on_action(Action::Search(query)))
    });

    let content = container(
        Column::new()
            .push(search)
            .push(
                scrollable(Column::with_children(items).width(Fill))
                    .id(LIST_ID)
                    .width(Fill),
            )
            .spacing(f32::from(MENU_PADDING)),
    )
    .width(Fill)
    .padding(MENU_PADDING)
    .style(menu_style);

    Open {
        content: content.into(),
        labels,
        keys,
    }
}

struct Dropdown<'a, Message> {
    anchor: Element<'a, Message>,
    open: Option<Open<'a, Message>>,
}

struct Open<'a, Message> {
    content: Element<'a, Message>,
    // every label, not only the matches, so the menu keeps its width while searching
    labels: Vec<String>,
    keys: Keys<Message>,
}

struct Keys<Message> {
    dismiss: Message,
    up: Option<Message>,
    down: Option<Message>,
    submit: Option<Message>,
}

impl<Message> Dropdown<'_, Message> {
    fn elements(&self) -> impl Iterator<Item = &Element<'_, Message>> {
        [
            Some(&self.anchor),
            self.open.as_ref().map(|open| &open.content),
        ]
        .into_iter()
        .flatten()
    }
}

impl<Message: Clone> Widget<Message, Theme, Renderer> for Dropdown<'_, Message> {
    fn size(&self) -> Size<iced::Length> {
        self.anchor.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        self.elements().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.elements().collect::<Vec<_>>());
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let open = self.open.as_mut()?;

        Some(overlay::Element::new(Box::new(Overlay {
            open,
            tree: &mut tree.children[1],
            anchor: layout.bounds() + translation,
        })))
    }
}

impl<'a, Message: Clone + 'a> From<Dropdown<'a, Message>> for Element<'a, Message> {
    fn from(dropdown: Dropdown<'a, Message>) -> Self {
        Element::new(dropdown)
    }
}

struct Overlay<'a, 'b, Message> {
    open: &'b mut Open<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
}

impl<Message: Clone> overlay::Overlay<Message, Theme, Renderer> for Overlay<'_, '_, Message> {
    // as wide as the longest entry but at least the anchor, below unless there is more room above
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
        let chrome = 2.0 * (f32::from(ITEM_PADDING[1]) + f32::from(MENU_PADDING) + BORDER_WIDTH)
            + SCROLLBAR_WIDTH;

        let longest = self
            .open
            .labels
            .iter()
            .map(|label| text_width(renderer, label))
            .fold(0.0, f32::max);

        let width = (longest + chrome).max(self.anchor.width).min(bounds.width);

        let below = bounds.height - self.anchor.y - self.anchor.height - MENU_GAP;
        let above = self.anchor.y - MENU_GAP;
        let height = MENU_MAX_HEIGHT.min(below.max(above)).max(0.0);

        let node = self.open.content.as_widget_mut().layout(
            self.tree,
            renderer,
            &Limits::new(Size::new(width, 0.0), Size::new(width, height)),
        );

        let y = match node.size().height <= below || below >= above {
            true => self.anchor.y + self.anchor.height + MENU_GAP,
            false => self.anchor.y - MENU_GAP - node.size().height,
        };

        let x = self.anchor.x.min(bounds.width - width).max(0.0);

        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.open.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        // handled before the search field sees them, it would otherwise swallow Escape and Enter
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key:
                Key::Named(named @ (Named::Escape | Named::ArrowUp | Named::ArrowDown | Named::Enter)),
            ..
        }) = event
        {
            let keys = &self.open.keys;

            let message = match named {
                Named::Escape => Some(&keys.dismiss),
                Named::ArrowUp => keys.up.as_ref(),
                Named::ArrowDown => keys.down.as_ref(),
                _ => keys.submit.as_ref(),
            };

            if let Some(message) = message {
                shell.publish(message.clone());
            }

            shell.capture_event();
            return;
        }

        // the anchor toggles the menu itself
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && !cursor.is_over(layout.bounds())
            && !cursor.is_over(self.anchor)
        {
            shell.publish(self.open.keys.dismiss.clone());
        }

        self.open.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let interaction = self.open.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        );

        // iced hands the cursor to the widgets below unless the overlay claims it, so the
        // menu's padding would otherwise let clicks and scrolling through to the form
        match interaction {
            mouse::Interaction::None if cursor.is_over(layout.bounds()) => mouse::Interaction::Idle,
            _ => interaction,
        }
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.open
            .content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }
}

fn text_width(renderer: &Renderer, content: &str) -> f32 {
    let paragraph = <Renderer as core_text::Renderer>::Paragraph::with_text(core_text::Text {
        content,
        bounds: Size::INFINITE,
        size: renderer.default_size(),
        line_height: core_text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: core_text::Alignment::Default,
        align_y: Vertical::Top,
        shaping: core_text::Shaping::default(),
        wrapping: core_text::Wrapping::None,
    });

    paragraph.min_width()
}

fn anchor_style(theme: &Theme, status: button::Status, is_open: bool) -> button::Style {
    let palette = theme.extended_palette();

    let border_color = match (is_open, status) {
        (true, _) | (_, button::Status::Hovered | button::Status::Pressed) => {
            palette.primary.strong.color
        }
        _ => palette.background.strong.color,
    };

    button::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: palette.background.weak.text,
        border: Border {
            color: border_color,
            width: BORDER_WIDTH,
            radius: ANCHOR_RADIUS.into(),
        },
        ..Default::default()
    }
}

fn item_style(theme: &Theme, status: button::Status, is_highlighted: bool) -> button::Style {
    let palette = theme.extended_palette();

    let (background, text_color) = match (is_highlighted, status) {
        (true, _) => (Some(palette.primary.weak.color), palette.primary.weak.text),
        (false, button::Status::Hovered | button::Status::Pressed) => (
            Some(palette.background.strong.color),
            palette.background.strong.text,
        ),
        (false, _) => (None, palette.background.base.text),
    };

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border::default().rounded(ANCHOR_RADIUS),
        ..Default::default()
    }
}

fn menu_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: Border {
            color: palette.background.strong.color,
            width: BORDER_WIDTH,
            radius: MENU_RADIUS.into(),
        },
        ..Default::default()
    }
}
