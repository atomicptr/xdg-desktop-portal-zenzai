use std::{f64::consts::FRAC_PI_2, sync::Arc};

use anyhow::{Result, anyhow};
use hayro::{
    RenderCache, RenderSettings,
    hayro_interpret::{InterpreterSettings, util::TransformExt},
    hayro_syntax::Pdf,
    kurbo::{Affine, Rect},
    render_into,
    vello_cpu::{
        Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit,
        color::palette::css::WHITE,
    },
};
use iced::widget::image::Handle;

use crate::settings::NumberUpLayout;

// longest edge of a rendered sheet in pixels, enough for a crisp preview at half the window
const SHEET_PIXELS: f64 = 900.0;

const POINTS_PER_MM: f64 = 72.0 / 25.4;

// also used as the preview cache key, so it must hold everything that changes how a sheet looks
#[derive(Debug, Clone, PartialEq)]
pub struct SheetSpec {
    pub pages: Vec<u32>,
    // None keeps the document's own page size, documents saved as is are not reformatted
    pub paper_mm: Option<(f64, f64)>,
    pub landscape: bool,
    // the last side may hold fewer pages but must keep the same grid
    pub number_up: usize,
    pub layout: NumberUpLayout,
    pub scale: f64,
    pub grayscale: bool,
}

pub struct Document {
    pdf: Pdf,
}

impl Document {
    pub fn open(data: Vec<u8>) -> Result<Self> {
        let pdf =
            Pdf::new(Arc::new(data)).map_err(|err| anyhow!("could not parse PDF: {err:?}"))?;

        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> u32 {
        self.pdf.pages().len() as u32
    }

    pub fn render(&self, spec: &SheetSpec) -> Handle {
        let paper_mm = spec
            .paper_mm
            .unwrap_or_else(|| self.page_size_mm(spec.pages.first()));

        // like CUPS, some n-up counts are laid out on a sideways sheet, landscape turns it again
        let (columns, rows, sideways) = nup_grid(spec.number_up);

        let (width_mm, height_mm) = match spec.landscape != sideways {
            true => (paper_mm.1, paper_mm.0),
            false => paper_mm,
        };

        let pixels_per_mm = SHEET_PIXELS / width_mm.max(height_mm);
        let width = (width_mm * pixels_per_mm) as u16;
        let height = (height_mm * pixels_per_mm) as u16;

        let mut ctx = RenderContext::new(width, height);
        let cache = RenderCache::new();
        let settings = InterpreterSettings::default();
        let (columns, rows) = match spec.landscape {
            true => (rows, columns),
            false => (columns, rows),
        };

        let cells = grid(
            spec.number_up,
            (columns, rows),
            (f64::from(width), f64::from(height)),
            spec.layout,
        );

        for (index, cell) in spec.pages.iter().zip(cells) {
            let Some(page) = self.pdf.pages().get(*index as usize) else {
                continue;
            };

            let (page_width, page_height) = page.render_dimensions();
            let (page_width, page_height) = (f64::from(page_width), f64::from(page_height));

            // like CUPS, pages whose shape does not match the cell are turned by 90 degrees
            let rotate = (page_width > page_height) != (cell.width() > cell.height());
            let (fit_width, fit_height) = match rotate {
                true => (page_height, page_width),
                false => (page_width, page_height),
            };

            let fit = (cell.width() / fit_width).min(cell.height() / fit_height) * spec.scale;
            let offset_x = cell.x0 + (cell.width() - fit_width * fit) / 2.0;
            let offset_y = cell.y0 + (cell.height() - fit_height * fit) / 2.0;

            // rotating counter-clockwise around the origin moves the page above it, shift it back down
            let rotation = match rotate {
                true => Affine::translate((0.0, page_width)) * Affine::rotate(-FRAC_PI_2),
                false => Affine::IDENTITY,
            };

            let transform = Affine::translate((offset_x, offset_y))
                * Affine::scale(fit)
                * rotation
                * page.initial_transform(true).to_kurbo();

            render_into(
                page,
                &cache,
                &settings,
                &RenderSettings::default(),
                &mut ctx,
                transform,
            );
        }

        ctx.flush();

        let mut pixmap = Pixmap::new(width, height);
        ctx.render_with(
            &mut pixmap,
            &mut Resources::default(),
            RasterizerSettings {
                target_init: TargetInit::Clear(WHITE),
                ..Default::default()
            },
        );

        // the background is opaque white, so premultiplied and straight alpha are identical
        let mut pixels = pixmap.data_as_u8_slice().to_vec();

        if spec.grayscale {
            pixels.as_chunks_mut::<4>().0.iter_mut().for_each(grayscale);
        }

        Handle::from_rgba(u32::from(width), u32::from(height), pixels)
    }
}

impl Document {
    fn page_size_mm(&self, index: Option<&u32>) -> (f64, f64) {
        let (width, height) = index
            .and_then(|index| self.pdf.pages().get(*index as usize))
            .map_or((1.0, 1.0), |page| page.render_dimensions());

        (
            f64::from(width) / POINTS_PER_MM,
            f64::from(height) / POINTS_PER_MM,
        )
    }
}

// columns, rows and whether the sheet is turned sideways, the presets of CUPS' pdftopdf
fn nup_grid(number_up: usize) -> (usize, usize, bool) {
    match number_up {
        2 => (2, 1, true),
        4 => (2, 2, false),
        6 => (3, 2, true),
        9 => (3, 3, false),
        16 => (4, 4, false),
        _ => (1, 1, false),
    }
}

// ITU-R BT.601 luma, what "grayscale" commonly means for print previews
fn grayscale([r, g, b, _]: &mut [u8; 4]) {
    let luma =
        (0.299 * f32::from(*r) + 0.587 * f32::from(*g) + 0.114 * f32::from(*b)).round() as u8;

    [*r, *g, *b] = [luma; 3];
}

fn grid(
    count: usize,
    (columns, rows): (usize, usize),
    (width, height): (f64, f64),
    layout: NumberUpLayout,
) -> Vec<Rect> {
    let (cell_width, cell_height) = (width / columns as f64, height / rows as f64);
    let (left_to_right, top_to_bottom, rows_first) = layout.directions();

    (0..count.max(1))
        .map(|index| {
            let (column, row) = match rows_first {
                true => (index % columns, index / columns),
                false => (index / rows, index % rows),
            };

            let column = if left_to_right {
                column
            } else {
                columns - 1 - column
            };
            let row = if top_to_bottom { row } else { rows - 1 - row };

            Rect::new(
                column as f64 * cell_width,
                row as f64 * cell_height,
                (column + 1) as f64 * cell_width,
                (row + 1) as f64 * cell_height,
            )
        })
        .collect()
}
