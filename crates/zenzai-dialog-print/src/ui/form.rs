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
const RANGES_PLACEHOLDER: &str = "e.g. 1-3, 7";
const MORE_SETTINGS: &str = "More settings";
const COLLAPSED: &str = "▸";
const EXPANDED: &str = "▾";
const LABEL_DESTINATION: &str = "Destination";
const LABEL_FILE: &str = "File";
const LABEL_PAGES: &str = "Pages";
const LABEL_COPIES: &str = "Copies";
const LABEL_ORIENTATION: &str = "Orientation";
const LABEL_COLOR: &str = "Color";
const OUTPUT_PLACEHOLDER: &str = "Output file";
const CHOOSE_LABEL: &str = "Choose…";
const LABEL_PAPER_SIZE: &str = "Paper size";
const LABEL_PAPER_TYPE: &str = "Paper type";
const LABEL_PAGES_PER_SHEET: &str = "Pages per sheet";
const LABEL_PAGE_ORDER: &str = "Page order";
const LABEL_TWO_SIDED: &str = "Two-sided";
const LABEL_QUALITY: &str = "Quality";
const LABEL_SCALE: &str = "Scale (%)";
const LABEL_ONLY_PRINT: &str = "Only print";
const LABEL_OPTIONS: &str = "Options";
const LABEL_REVERSE: &str = "Reverse order";
const LABEL_COLLATE: &str = "Collate copies";
const RENDERING: &str = "Rendering…";
const PREVIEW_LABEL: &str = "Preview";
pub(super) const PREVIEW_UNAVAILABLE: &str = "Preview unavailable";
const INLINE_PREVIEW_HEIGHT: f32 = 320.0;
const PREVIOUS: &str = "‹";
const NEXT: &str = "›";
const FRONT: &str = "front";
const BACK: &str = "back";

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
        LABEL_DESTINATION,
        &state.destinations,
        state.selected.as_ref(),
        Message::SelectDestination,
    );

    let mut form = column![header, form_row(LABEL_DESTINATION, destination)].spacing(FORM_SPACING);

    if state.is_pdf() {
        form = form.push(form_row(LABEL_FILE, output(state)));
    }

    if !state.page_options_apply() {
        return form;
    }

    let orientation = segmented(Orientation::ALL, &current.orientation, |orientation| {
        edit(state, move |s| s.orientation = orientation)
    });

    form = form
        .push(form_row(LABEL_PAGES, pages(state)))
        .push(form_row(
            LABEL_COPIES,
            container(stepper(current.copies, 1, MAX_COPIES, |copies| {
                edit(state, move |s| s.copies = copies)
            }))
            .width(CONTROL_WIDTH),
        ))
        .push(form_row(
            LABEL_ORIENTATION,
            container(orientation).width(CONTROL_WIDTH),
        ));

    if caps.is_some_and(|caps| caps.color) {
        form = form.push(form_row(
            LABEL_COLOR,
            picker(
                state,
                LABEL_COLOR,
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
        MORE_SETTINGS,
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
        text_input(OUTPUT_PLACEHOLDER, &display)
            .on_input(Message::OutputTyped)
            .width(Fill)
            .into()
    } else {
        text(display).width(Fill).into()
    };

    let choose = button(text(CHOOSE_LABEL))
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
            LABEL_PAGES,
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
            text_input(RANGES_PLACEHOLDER, &current.ranges)
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
        LABEL_PAPER_SIZE,
        picker(
            state,
            LABEL_PAPER_SIZE,
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
            LABEL_PAPER_TYPE,
            picker(
                state,
                LABEL_PAPER_TYPE,
                media_types,
                selected,
                |choice: Choice| edit(state, move |s| s.media_type = Some(choice.value)),
            ),
        ));
    }

    form = form.push(form_row(
        LABEL_PAGES_PER_SHEET,
        container(dropdown(
            LABEL_PAGES_PER_SHEET,
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
            LABEL_PAGE_ORDER,
            picker(
                state,
                LABEL_PAGE_ORDER,
                NumberUpLayout::ALL,
                Some(&current.number_up_layout),
                |layout| edit(state, move |s| s.number_up_layout = layout),
            ),
        ));
    }

    if caps.is_some_and(|caps| caps.duplex) {
        form = form.push(form_row(
            LABEL_TWO_SIDED,
            picker(
                state,
                LABEL_TWO_SIDED,
                Duplex::ALL,
                Some(&current.duplex),
                |duplex| edit(state, move |s| s.duplex = duplex),
            ),
        ));
    }

    if caps.is_some_and(|caps| caps.quality) {
        form = form.push(form_row(
            LABEL_QUALITY,
            picker(
                state,
                LABEL_QUALITY,
                Quality::ALL,
                Some(&current.quality),
                |quality| edit(state, move |s| s.quality = quality),
            ),
        ));
    }

    // CUPS can not scale existing documents, only applications rendering for us apply it
    if state.dialog.document.is_none() {
        form = form.push(form_row(
            LABEL_SCALE,
            container(stepper(current.scale, MIN_SCALE, MAX_SCALE, |scale| {
                edit(state, move |s| s.scale = scale)
            }))
            .width(CONTROL_WIDTH),
        ));
    }

    form.push(form_row(
        LABEL_ONLY_PRINT,
        picker(
            state,
            LABEL_ONLY_PRINT,
            PageSet::ALL,
            Some(&current.page_set),
            |page_set| edit(state, move |s| s.page_set = page_set),
        ),
    ))
    .push(form_row(
        LABEL_OPTIONS,
        column![
            checkbox(current.reverse)
                .label(LABEL_REVERSE)
                .on_toggle(move |reverse| edit(state, move |s| s.reverse = reverse)),
            checkbox(current.collate)
                .label(LABEL_COLLATE)
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
        Some(Preview::Rendering) => text(RENDERING).style(text::secondary).into(),
        Some(Preview::Failed) | None => text(PREVIEW_UNAVAILABLE).style(text::secondary).into(),
    };

    if state.can_preview() {
        sheet = column![
            sheet,
            button(text(PREVIEW_LABEL))
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
    let position = format!("{} / {count}", (side + 1).min(count));

    if !state.page_options_apply() || state.dialog.settings.duplex == Duplex::Off {
        return position;
    }

    let face = match side % 2 {
        0 => FRONT,
        _ => BACK,
    };

    format!("{position} · sheet {} {face}", side / 2 + 1)
}
