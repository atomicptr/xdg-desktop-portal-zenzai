use iced::{
    Alignment, Background, Border, Element, FillPortion, Length, Theme,
    widget::{button, container, row, text, text_input},
};

pub const PANE_PADDING: u16 = 16;
pub const PANE_SPACING: u32 = 24;
const SEGMENT_PADDING: [u16; 2] = [6, 14];
const SEGMENT_RADIUS: f32 = 6.0;
const SEGMENT_SPACING: u32 = 2;
const TRACK_PADDING: u16 = 3;
const TRACK_RADIUS: f32 = 8.0;
const LABEL_WIDTH: f32 = 130.0;
const STEPPER_INPUT_WIDTH: f32 = 70.0;
const STEPPER_SPACING: u32 = 4;
const FORM_ROW_SPACING: u32 = 12;
const ACTION_SPACING: u32 = 8;

pub fn segmented<'a, T, Message>(
    options: &[T],
    selected: &T,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Clone + PartialEq + ToString,
    Message: Clone + 'a,
{
    let segments = options.iter().map(|option| {
        let is_selected = option == selected;

        button(text(option.to_string()))
            .padding(SEGMENT_PADDING)
            .style(move |theme, status| segment_style(theme, status, is_selected))
            .on_press(on_select(option.clone()))
            .into()
    });

    container(row(segments).spacing(SEGMENT_SPACING))
        .padding(TRACK_PADDING)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(
                theme.extended_palette().background.weak.color,
            )),
            border: Border::default().rounded(TRACK_RADIUS),
            ..Default::default()
        })
        .into()
}

fn segment_style(theme: &Theme, status: button::Status, is_selected: bool) -> button::Style {
    let palette = theme.extended_palette();

    let (background, text_color) = match (is_selected, status) {
        (true, _) => (
            Some(palette.background.base.color),
            palette.background.base.text,
        ),
        (false, button::Status::Hovered | button::Status::Pressed) => (
            Some(palette.background.strong.color),
            palette.background.weak.text,
        ),
        (false, _) => (None, palette.background.weak.text),
    };

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border::default().rounded(SEGMENT_RADIUS),
        ..Default::default()
    }
}

// the halves match a two pane layout with the same padding and spacing, so messages wrap
// at the width of the left pane and grow the bar instead of squeezing the buttons
pub fn action_bar<'a, Message: Clone + 'a>(
    status: Option<Element<'a, Message>>,
    cancel_label: &'a str,
    on_cancel: Message,
    accept_label: &'a str,
    on_accept: Option<Message>,
) -> Element<'a, Message> {
    let buttons = row![
        button(text(cancel_label))
            .style(button::secondary)
            .on_press(on_cancel),
        button(text(accept_label))
            .style(button::primary)
            .on_press_maybe(on_accept),
    ]
    .spacing(ACTION_SPACING);

    row![
        container(status).width(FillPortion(1)),
        container(buttons).align_right(FillPortion(1)),
    ]
    .spacing(PANE_SPACING)
    .align_y(Alignment::Center)
    .padding(PANE_PADDING)
    .into()
}

pub fn form_row<'a, Message: 'a>(
    label: &'a str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(text(label)).width(Length::Fixed(LABEL_WIDTH)),
        control.into()
    ]
    .spacing(FORM_ROW_SPACING)
    .align_y(Alignment::Center)
    .into()
}

// typed values are only capped at `max`, clamping to `min` on every keystroke would make it
// impossible to clear the field or type multi-digit numbers, callers validate the minimum
pub fn stepper<'a, Message: Clone + 'a>(
    value: u32,
    min: u32,
    max: u32,
    on_change: impl Fn(u32) -> Message + 'a,
) -> Element<'a, Message> {
    let decrement = (value > min).then(|| on_change(value - 1));
    let increment = (value < max).then(|| on_change(value + 1));

    row![
        text_input("", &value.to_string())
            .on_input(move |input| on_change(input.parse().unwrap_or(0).min(max)))
            .width(Length::Fixed(STEPPER_INPUT_WIDTH)),
        button(text("−"))
            .style(button::secondary)
            .on_press_maybe(decrement),
        button(text("+"))
            .style(button::secondary)
            .on_press_maybe(increment),
    ]
    .spacing(STEPPER_SPACING)
    .align_y(Alignment::Center)
    .into()
}
