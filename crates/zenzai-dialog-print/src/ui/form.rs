use rust_i18n::t;

use iced::{
    Alignment, ContentFit, Element, Fill, Length,
    widget::{Column, button, checkbox, column, container, image, row, rule, text, text_input},
};
use zenzai_core::ui::{
    dropdown::{Search, dropdown},
    widgets::{form_row, segmented, stepper},
};

use crate::{
    cups::Choice,
    settings::{
        ColorMode, Duplex, JobSettings, MAX_COPIES, MAX_SCALE, MIN_SCALE, NUMBER_UP_CHOICES,
        NumberUpLayout, Orientation, PageSelection, PageSet, Quality,
    },
};

use super::{
    app::{Message, Preview, State},
    output,
};

const FORM_SPACING: u32 = 12;
const CONTROL_WIDTH: f32 = 240.0;
const PAGES_PICKER_WIDTH: f32 = 110.0;
const PAGES_SPACING: f32 = 8.0;
const TITLE_SIZE: u32 = 20;
const PREVIEW_PADDING: u16 = 12;
const COLLAPSED: &str = "▸";
const EXPANDED: &str = "▾";
const INLINE_PREVIEW_HEIGHT: f32 = 320.0;
const PREVIOUS: &str = "‹";
const NEXT: &str = "›";

fn edit(state: &State, f: impl FnOnce(&mut JobSettings)) -> Message {
    let mut settings = state.dialog.settings.clone();
    f(&mut settings);
    Message::Settings(settings)
}

fn picker<'a, T>(
    state: &'a State,
    id: &'static str,
    options: &[T],
    selected: Option<&T>,
    on_select: impl Fn(T) -> Message,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone,
{
    container(dropdown(
        id,
        options,
        selected,
        state.menu.as_ref(),
        Search::Auto,
        Message::Dropdown,
        on_select,
    ))
    .width(CONTROL_WIDTH)
    .into()
}

pub fn settings(state: &State) -> Element<'_, Message> {
    let mut form = fields(state);

    if state.compact() {
        form = form.push(container(preview(state)).height(INLINE_PREVIEW_HEIGHT));
    }

    form.into()
}

fn fields(state: &State) -> Column<'_, Message> {
    let current = &state.dialog.settings;
    let caps = state.capabilities.as_ref();

    let header = row![
        text(&state.dialog.title).size(TITLE_SIZE).width(Fill),
        text(state.subtitle().unwrap_or_default()).style(text::secondary),
    ]
    .align_y(Alignment::Center);

    let destination = picker(
        state,
        "Destination",
        &state.destinations,
        state.selected.as_ref(),
        Message::SelectDestination,
    );

    let mut form =
        column![header, form_row(t!("print.destination"), destination)].spacing(FORM_SPACING);

    if state.is_pdf() {
        form = form.push(form_row(t!("print.file"), output(state)));
    }

    if !state.page_options_apply() {
        return form;
    }

    let orientation = segmented(Orientation::ALL, &current.orientation, |orientation| {
        edit(state, move |s| s.orientation = orientation)
    });

    form = form
        .push(form_row(t!("print.pages"), pages(state)))
        .push(form_row(
            t!("print.copies"),
            container(stepper(current.copies, 1, MAX_COPIES, |copies| {
                edit(state, move |s| s.copies = copies)
            }))
            .width(CONTROL_WIDTH),
        ))
        .push(form_row(
            t!("print.orientation"),
            container(orientation).width(CONTROL_WIDTH),
        ));

    if caps.is_some_and(|caps| caps.color) {
        form = form.push(form_row(
            t!("print.color"),
            picker(
                state,
                "Color",
                ColorMode::ALL,
                Some(&current.color),
                |color| edit(state, move |s| s.color = color),
            ),
        ));
    }

    form = form.push(rule::horizontal(1)).push(more_toggle(state));

    if state.more_settings {
        form = form.push(more_settings(state));
    }

    form
}

fn more_toggle(state: &State) -> Element<'_, Message> {
    let arrow = if state.more_settings {
        EXPANDED
    } else {
        COLLAPSED
    };

    button(form_row(
        t!("print.more-settings"),
        container(text(arrow)).align_right(CONTROL_WIDTH),
    ))
    .style(button::text)
    .padding(0)
    .on_press(Message::ToggleMoreSettings)
    .into()
}

fn output(state: &State) -> Element<'_, Message> {
    let display = output::display(&state.dialog.output);
    let path: Element<'_, Message> = if state.manual_output {
        text_input(&t!("print.output-file"), &display)
            .on_input(Message::OutputTyped)
            .width(Fill)
            .into()
    } else {
        text(display).width(Fill).into()
    };

    let choose = button(text(t!("common.choose")))
        .style(button::secondary)
        .on_press_maybe((!state.picking_output).then_some(Message::ChooseOutput));

    row![path, choose]
        .spacing(PAGES_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fixed(CONTROL_WIDTH))
        .into()
}

fn pages(state: &State) -> Element<'_, Message> {
    let current = &state.dialog.settings;

    let choices: Vec<_> = PageSelection::ALL
        .iter()
        .copied()
        .filter(|choice| match choice {
            PageSelection::Current => state.dialog.has_current_page,
            PageSelection::Selection => state.dialog.has_selected_pages,
            _ => true,
        })
        .collect();

    let mut pages = row![
        container(dropdown(
            "Pages",
            &choices,
            Some(&current.pages),
            state.menu.as_ref(),
            Search::Auto,
            Message::Dropdown,
            |pages| edit(state, move |s| s.pages = pages),
        ))
        .width(if current.pages == PageSelection::Ranges {
            PAGES_PICKER_WIDTH
        } else {
            CONTROL_WIDTH
        })
    ]
    .spacing(PAGES_SPACING)
    .align_y(Alignment::Center)
    .width(CONTROL_WIDTH);

    if current.pages == PageSelection::Ranges {
        pages = pages.push(
            text_input(&t!("print.page-range-placeholder"), &current.ranges)
                .on_input(move |ranges| edit(state, move |s| s.ranges = ranges))
                .width(Length::Fixed(
                    CONTROL_WIDTH - PAGES_PICKER_WIDTH - PAGES_SPACING,
                )),
        );
    }

    pages.into()
}

fn more_settings(state: &State) -> Element<'_, Message> {
    let current = &state.dialog.settings;
    let caps = state.capabilities.as_ref();

    let mut form = column![form_row(
        t!("print.paper-size"),
        picker(
            state,
            "Paper size",
            &state.papers(),
            current.paper.as_ref(),
            |paper| { edit(state, move |s| s.paper = Some(paper)) }
        ),
    )]
    .spacing(FORM_SPACING);

    if let Some(media_types) = caps.map(|caps| &caps.media_types).filter(|t| !t.is_empty()) {
        let selected = media_types
            .iter()
            .find(|choice| current.media_type.as_ref() == Some(&choice.value));

        form = form.push(form_row(
            t!("print.paper-type"),
            picker(
                state,
                "Paper type",
                media_types,
                selected,
                |choice: Choice| edit(state, move |s| s.media_type = Some(choice.value)),
            ),
        ));
    }

    form = form.push(form_row(
        t!("print.pages-per-sheet"),
        container(dropdown(
            "Pages per sheet",
            &NUMBER_UP_CHOICES,
            Some(&current.number_up),
            state.menu.as_ref(),
            Search::Off,
            Message::Dropdown,
            |n| edit(state, move |s| s.number_up = n),
        ))
        .width(CONTROL_WIDTH),
    ));

    if current.number_up > 1 {
        form = form.push(form_row(
            t!("print.page-order"),
            picker(
                state,
                "Page order",
                NumberUpLayout::ALL,
                Some(&current.number_up_layout),
                |layout| edit(state, move |s| s.number_up_layout = layout),
            ),
        ));
    }

    if caps.is_some_and(|caps| caps.duplex) {
        form = form.push(form_row(
            t!("print.two-sided"),
            picker(
                state,
                "Two-sided",
                Duplex::ALL,
                Some(&current.duplex),
                |duplex| edit(state, move |s| s.duplex = duplex),
            ),
        ));
    }

    if caps.is_some_and(|caps| caps.quality) {
        form = form.push(form_row(
            t!("print.quality"),
            picker(
                state,
                "Quality",
                Quality::ALL,
                Some(&current.quality),
                |quality| edit(state, move |s| s.quality = quality),
            ),
        ));
    }

    // CUPS can not scale existing documents, only applications rendering for us apply it
    if state.dialog.document.is_none() {
        form = form.push(form_row(
            t!("print.scale"),
            container(stepper(current.scale, MIN_SCALE, MAX_SCALE, |scale| {
                edit(state, move |s| s.scale = scale)
            }))
            .width(CONTROL_WIDTH),
        ));
    }

    form.push(form_row(
        t!("print.only-print"),
        picker(
            state,
            "Only print",
            PageSet::ALL,
            Some(&current.page_set),
            |page_set| edit(state, move |s| s.page_set = page_set),
        ),
    ))
    .push(form_row(
        t!("print.options"),
        column![
            checkbox(current.reverse)
                .label(t!("print.reverse-order"))
                .on_toggle(move |reverse| edit(state, move |s| s.reverse = reverse)),
            checkbox(current.collate)
                .label(t!("print.collate-copies"))
                .on_toggle(move |collate| edit(state, move |s| s.collate = collate)),
        ]
        .spacing(PAGES_SPACING),
    ))
    .into()
}

pub fn preview(state: &State) -> Element<'_, Message> {
    let sides = state.sides();

    let mut sheet: Element<'_, Message> = match state.preview(&sides) {
        Some(Preview::Ready(handle)) => image(handle.clone())
            .content_fit(ContentFit::Contain)
            .width(Fill)
            .height(Fill)
            .into(),
        Some(Preview::Rendering) => text(t!("print.rendering")).style(text::secondary).into(),
        Some(Preview::Failed) | None => text(t!("print.preview-unavailable"))
            .style(text::secondary)
            .into(),
    };

    if state.can_preview() {
        sheet = column![
            sheet,
            button(text(t!("print.preview")))
                .style(button::secondary)
                .on_press_maybe(state.can_accept().then_some(Message::RequestPreview)),
        ]
        .spacing(FORM_SPACING)
        .align_x(Alignment::Center)
        .into();
    }

    let sheet = container(sheet)
        .center(Fill)
        .padding(PREVIEW_PADDING)
        .style(container::rounded_box);

    if sides.is_empty() {
        return sheet.into();
    }

    let side = state.preview_side;

    let navigation = row![
        button(text(PREVIOUS))
            .style(button::secondary)
            .on_press_maybe(side.checked_sub(1).map(Message::PreviewSide)),
        text(side_label(state, side, sides.len())),
        button(text(NEXT))
            .style(button::secondary)
            .on_press_maybe((side + 1 < sides.len()).then_some(Message::PreviewSide(side + 1))),
    ]
    .spacing(PAGES_SPACING)
    .align_y(Alignment::Center);

    column![sheet, container(navigation).center_x(Fill),]
        .spacing(PAGES_SPACING)
        .into()
}

fn side_label(state: &State, side: usize, count: usize) -> String {
    let position = t!(
        "print.preview.position",
        current = (side + 1).min(count),
        count = count
    );

    if !state.page_options_apply() || state.dialog.settings.duplex == Duplex::Off {
        return position.into_owned();
    }

    let sheet = side / 2 + 1;

    if side.is_multiple_of(2) {
        t!(
            "print.preview.sheet-front",
            position = position,
            sheet = sheet
        )
    } else {
        t!(
            "print.preview.sheet-back",
            position = position,
            sheet = sheet
        )
    }
    .into_owned()
}
