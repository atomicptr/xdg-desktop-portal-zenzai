use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use data_url::DataUrl;
use iced::{
    advanced::graphics::text::FIRA_SANS_REGULAR,
    widget::{image as raster, svg},
};
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use usvg::{
    ImageHrefResolver, ImageKind, Options, Tree, WriteOptions,
    roxmltree::{Document, ParsingOptions},
};

const MAX_DIMENSION: u32 = 512;
const MAX_DECODE_BYTES: u64 = 16 * 1024 * 1024;
const FALLBACK_FONT: &str = "Fira Sans";

#[derive(Debug, Clone)]
pub enum Icon {
    Raster(raster::Handle),
    Vector(svg::Handle),
}

fn decode_raster(bytes: &[u8], format: ImageFormat) -> Result<RgbaImage> {
    ensure!(
        bytes.len() as u64 <= MAX_DECODE_BYTES,
        "icon data is too large"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    Ok(reader
        .decode()
        .context("invalid launcher icon")?
        .into_rgba8())
}

fn svg_options() -> Options<'static> {
    let mut options = Options {
        font_family: FALLBACK_FONT.into(),
        ..Options::default()
    };
    options
        .fontdb_mut()
        .load_font_data(FIRA_SANS_REGULAR.to_vec());
    options.fontdb_mut().set_serif_family(FALLBACK_FONT);
    options
}

fn sanitize_svg(source: &str, mut options: Options<'_>) -> Result<String> {
    let document = Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        },
    )
    .context("invalid SVG icon")?;
    // usvg skips malformed base64 before invoking its image resolver.
    for node in document
        .descendants()
        .filter(|node| matches!(node.tag_name().name(), "image" | "feImage"))
    {
        for attribute in node
            .attributes()
            .filter(|attribute| attribute.name() == "href")
        {
            if let Ok(url) = DataUrl::process(attribute.value()) {
                ensure!(url.decode_to_vec().is_ok(), "invalid SVG image data URL");
            }
        }
    }

    let rejected = Arc::new(AtomicBool::new(false));
    let data_rejected = Arc::clone(&rejected);
    let external_rejected = Arc::clone(&rejected);
    options.image_href_resolver = ImageHrefResolver {
        resolve_data: Box::new(move |_, data, _| match image::guess_format(&data) {
            Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg))
                if decode_raster(&data, format).is_ok() =>
            {
                Some(match format {
                    ImageFormat::Png => ImageKind::PNG(data),
                    _ => ImageKind::JPEG(data),
                })
            }
            _ => {
                data_rejected.store(true, Ordering::Relaxed);
                None
            }
        }),
        resolve_string: Box::new(move |_, _| {
            external_rejected.store(true, Ordering::Relaxed);
            None
        }),
    };
    let tree = Tree::from_xmltree(&document, &options).context("invalid SVG icon")?;
    ensure!(
        !rejected.load(Ordering::Relaxed),
        "SVG contains an external, unsupported, or oversized image"
    );
    Ok(tree.to_string(&WriteOptions::default()))
}

pub fn decode(bytes: &[u8]) -> Result<Option<Icon>> {
    if bytes.is_empty() {
        return Ok(None);
    }

    if let Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg)) = image::guess_format(bytes) {
        let rgba = decode_raster(bytes, format)?;
        return Ok(Some(Icon::Raster(raster::Handle::from_rgba(
            rgba.width(),
            rgba.height(),
            rgba.into_raw(),
        ))));
    }

    let source = std::str::from_utf8(bytes).context("icon must be PNG, JPEG, or SVG")?;
    let mut options = svg_options();
    options.fontdb_mut().load_system_fonts();
    let sanitized = sanitize_svg(source, options)?;
    Ok(Some(Icon::Vector(svg::Handle::from_memory(
        sanitized.into_bytes(),
    ))))
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;
    use std::fmt::Write;

    use super::*;

    #[test]
    fn svg_text_is_preserved_as_paths_without_system_fonts() {
        for background in ["", r#"<path d="M0 0h64v64H0z" fill="white"/>"#] {
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">{background}<text x="8" y="40" font-family="UnavailableTestFont">A</text></svg>"#
            );
            let sanitized = sanitize_svg(&source, svg_options()).unwrap();
            let tree = Tree::from_str(&sanitized, &Options::default()).unwrap();
            assert!(!tree.root().children().is_empty());
            assert!(sanitized.matches("<path").count() > usize::from(!background.is_empty()));
        }
    }

    #[test]
    fn svg_embedded_rasters_are_preserved_and_bounded() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            for width in [1, MAX_DIMENSION + 1] {
                let mut bytes = Cursor::new(Vec::new());
                DynamicImage::new_rgb8(width, 1)
                    .write_to(&mut bytes, format)
                    .unwrap();
                let data = bytes
                    .into_inner()
                    .iter()
                    .fold(String::new(), |mut data, byte| {
                        write!(data, "%{byte:02X}").unwrap();
                        data
                    });
                let source = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="64" height="64"><image width="64" height="64" xlink:href="data:image/{},{data}"/></svg>"#,
                    if format == ImageFormat::Png {
                        "png"
                    } else {
                        "jpeg"
                    }
                );
                let result = sanitize_svg(&source, svg_options());
                if width == 1 {
                    assert!(result.unwrap().contains("<image"));
                } else {
                    assert!(result.is_err());
                }
            }
        }
    }

    #[test]
    fn svg_external_and_unsupported_images_are_rejected() {
        for href in [
            "/tmp/icon.png",
            "https://example.com/icon.png",
            "data:image/gif,GIF89a",
            "data:image/png;base64,!",
            "data:image/png;base64,a",
        ] {
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="64" height="64"><image width="64" height="64" xlink:href="{href}"/></svg>"#
            );
            assert!(sanitize_svg(&source, svg_options()).is_err());
        }
    }
}
