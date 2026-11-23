use std::{
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
    str,
};

use iced::{
    Element, Theme,
    widget::{image as raster, svg},
};
use image::{ImageFormat, ImageReader, Limits};

const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ICON_DIMENSION: u32 = 1024;

#[derive(Clone)]
pub enum Icon {
    Svg(svg::Handle, bool),
    Png(raster::Handle),
}

#[must_use]
pub fn load(name: &str, size: u16, theme: Option<&str>) -> Option<Icon> {
    let path = lookup(name, size, theme)?;
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

impl Icon {
    pub fn view<Message>(&self, size: impl Into<iced::Length> + Copy) -> Element<'_, Message> {
        match self {
            Self::Svg(handle, symbolic) => svg(handle.clone())
                .width(size)
                .height(size)
                .style(move |theme: &Theme, _| svg::Style {
                    color: symbolic.then_some(theme.palette().text),
                })
                .into(),
            Self::Png(handle) => raster(handle.clone()).width(size).height(size).into(),
        }
    }
}

#[must_use]
pub fn lookup(name: &str, size: u16, theme: Option<&str>) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return None;
    }

    let lookup = freedesktop_icons::lookup(name).with_size(size);
    let lookup = if let Some(theme) = theme {
        lookup.with_theme(theme)
    } else {
        lookup
    };
    lookup.find()
}
