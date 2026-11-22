use std::{fmt, path::Path};

use clap::ValueEnum;
use zenzai_core::ipc::{PageSetup, PrintSettings, SetupValue};

use url::Url;

use crate::paper::Paper;

pub mod key {
    pub const ORIENTATION: &str = "orientation";
    pub const PAPER_FORMAT: &str = "paper-format";
    pub const PAPER_WIDTH: &str = "paper-width";
    pub const PAPER_HEIGHT: &str = "paper-height";
    pub const N_COPIES: &str = "n-copies";
    pub const QUALITY: &str = "quality";
    pub const USE_COLOR: &str = "use-color";
    pub const DUPLEX: &str = "duplex";
    pub const COLLATE: &str = "collate";
    pub const REVERSE: &str = "reverse";
    pub const MEDIA_TYPE: &str = "media-type";
    pub const SCALE: &str = "scale";
    pub const PRINT_PAGES: &str = "print-pages";
    pub const PAGE_RANGES: &str = "page-ranges";
    pub const PAGE_SET: &str = "page-set";
    pub const NUMBER_UP: &str = "number-up";
    pub const NUMBER_UP_LAYOUT: &str = "number-up-layout";
    pub const OUTPUT_FILE_FORMAT: &str = "output-file-format";
    pub const OUTPUT_URI: &str = "output-uri";
    pub const PRINTER: &str = "printer";
}

pub mod ipp {
    pub const COPIES: &str = "copies";
    pub const SIDES: &str = "sides";
    pub const COLOR_MODE: &str = "print-color-mode";
    pub const QUALITY: &str = "print-quality";
    pub const DOCUMENT_HANDLING: &str = "multiple-document-handling";
    pub const MEDIA: &str = "media";
    pub const MEDIA_TYPE: &str = "media-type";
    pub const NUMBER_UP: &str = "number-up";
    pub const NUMBER_UP_LAYOUT: &str = "number-up-layout";
    pub const ORIENTATION: &str = "orientation-requested";
    // selects document pages, `page-ranges` would count output sides once n-up is applied
    pub const PAGE_RANGES: &str = "input-page-ranges";
    pub const PAGE_SET: &str = "page-set";
    pub const OUTPUT_ORDER: &str = "output-order";
    pub const PRINT_SCALING: &str = "print-scaling";

    pub const COLLATED: &str = "separate-documents-collated-copies";
    pub const UNCOLLATED: &str = "separate-documents-uncollated-copies";
    pub const REVERSE_ORDER: &str = "reverse";
    pub const SCALE_TO_FIT: &str = "fit";
}

mod setup {
    pub const NAME: &str = "Name";
    pub const DISPLAY_NAME: &str = "DisplayName";
    pub const WIDTH: &str = "Width";
    pub const HEIGHT: &str = "Height";
    pub const ORIENTATION: &str = "Orientation";
    pub const PPD_NAME: &str = "PPDName";
}

const TRUE: &str = "true";
pub const PDF_FORMAT: &str = "pdf";
const DEFAULT_SCALE: u32 = 100;
pub const MIN_SCALE: u32 = 10;
pub const MAX_SCALE: u32 = 400;

pub const MAX_COPIES: u32 = 999;
pub const NUMBER_UP_CHOICES: [u32; 6] = [1, 2, 4, 6, 9, 16];

// each option: variant => portal value, IPP value, label; extra portal spellings are accepted
macro_rules! options {
    ($(#[$attr:meta])* $name:ident {
        $($(#[$vattr:meta])* $variant:ident => $portal:literal $(| $alias:literal)*, $ipp:literal, $label:literal;)+
    }) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub enum $name {
            $($(#[$vattr])* $variant,)+
        }

        #[allow(dead_code)]
        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant),+];

            fn portal(self) -> &'static str {
                match self { $(Self::$variant => $portal,)+ }
            }

            fn from_portal(value: &str) -> Option<Self> {
                match value { $($portal $(| $alias)* => Some(Self::$variant),)+ _ => None }
            }

            fn ipp(self) -> &'static str {
                match self { $(Self::$variant => $ipp,)+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(match self { $(Self::$variant => $label,)+ })
            }
        }
    };
}

options!(
    #[derive(ValueEnum)]
    Orientation {
        #[default]
        Portrait => "portrait" | "reverse_portrait" | "reverse-portrait", "3", "Portrait";
        Landscape => "landscape" | "reverse_landscape" | "reverse-landscape", "4", "Landscape";
    }
);

// matches GTK: "horizontal" is long edge (DuplexNoTumble), "vertical" short edge (DuplexTumble)
options!(
    #[derive(ValueEnum)]
    Duplex {
        #[default]
        Off => "simplex", "one-sided", "Off";
        LongEdge => "horizontal", "two-sided-long-edge", "Long edge";
        ShortEdge => "vertical", "two-sided-short-edge", "Short edge";
    }
);

// the portal has no value for automatic color, it is left out of the settings
options!(
    #[derive(ValueEnum)]
    ColorMode {
        #[default]
        Auto => "", "auto", "Automatic";
        Color => "true", "color", "Color";
        Mono => "false", "monochrome", "Grayscale";
    }
);

options!(Quality {
    Draft => "draft" | "low", "3", "Draft";
    #[default]
    Normal => "normal", "4", "Normal";
    High => "high", "5", "High";
});

options!(PageSet {
    #[default]
    All => "all", "", "All pages";
    Odd => "odd", "odd", "Odd pages";
    Even => "even", "even", "Even pages";
});

options!(NumberUpLayout {
    #[default]
    Lrtb => "lrtb", "lrtb", "Left to right, top to bottom";
    Lrbt => "lrbt", "lrbt", "Left to right, bottom to top";
    Rltb => "rltb", "rltb", "Right to left, top to bottom";
    Rlbt => "rlbt", "rlbt", "Right to left, bottom to top";
    Tblr => "tblr", "tblr", "Top to bottom, left to right";
    Tbrl => "tbrl", "tbrl", "Top to bottom, right to left";
    Btlr => "btlr", "btlr", "Bottom to top, left to right";
    Btrl => "btrl", "btrl", "Bottom to top, right to left";
});

impl NumberUpLayout {
    // horizontal direction, vertical direction and whether rows are filled before columns
    pub fn directions(self) -> (bool, bool, bool) {
        match self {
            Self::Lrtb => (true, true, true),
            Self::Lrbt => (true, false, true),
            Self::Rltb => (false, true, true),
            Self::Rlbt => (false, false, true),
            Self::Tblr => (true, true, false),
            Self::Tbrl => (false, true, false),
            Self::Btlr => (true, false, false),
            Self::Btrl => (false, false, false),
        }
    }
}

options!(PageSelection {
    #[default]
    All => "all", "", "All";
    Current => "current", "", "Current page";
    Selection => "selection", "", "Selection";
    Ranges => "ranges", "", "Range";
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub start: u32,
    pub end: u32,
}

pub fn parse_ranges(input: &str) -> Option<Vec<PageRange>> {
    input
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (start, end) = part.split_once('-').unwrap_or((part, part));
            let start: u32 = start.trim().parse().ok()?;
            let end: u32 = end.trim().parse().ok()?;

            (start >= 1 && end >= start).then_some(PageRange { start, end })
        })
        .collect::<Option<Vec<_>>>()
        .filter(|ranges| !ranges.is_empty())
}

fn format_ranges(ranges: impl IntoIterator<Item = PageRange>) -> String {
    ranges
        .into_iter()
        .map(|range| {
            if range.start == range.end {
                range.start.to_string()
            } else {
                format!("{}-{}", range.start, range.end)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

// the portal transmits page ranges 0-based
fn ranges_from_portal(value: &str) -> Option<Vec<PageRange>> {
    value
        .split(',')
        .map(|part| {
            let (start, end) = part.split_once('-').unwrap_or((part, part));
            let start = start.trim().parse::<u32>().ok()?.checked_add(1)?;
            let end = end.trim().parse::<u32>().ok()?.checked_add(1)?;

            (end >= start).then_some(PageRange { start, end })
        })
        .collect()
}

fn ranges_to_portal(ranges: &[PageRange]) -> String {
    format_ranges(ranges.iter().map(|range| PageRange {
        start: range.start - 1,
        end: range.end - 1,
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobSettings {
    pub paper: Option<Paper>,
    pub orientation: Orientation,
    pub copies: u32,
    pub collate: bool,
    pub pages: PageSelection,
    pub ranges: String,
    pub duplex: Duplex,
    pub color: ColorMode,
    pub quality: Quality,
    pub number_up: u32,
    pub number_up_layout: NumberUpLayout,
    pub page_set: PageSet,
    pub reverse: bool,
    pub scale: u32,
    pub media_type: Option<String>,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            paper: None,
            orientation: Orientation::default(),
            copies: 1,
            collate: true,
            pages: PageSelection::default(),
            ranges: String::new(),
            duplex: Duplex::default(),
            color: ColorMode::default(),
            quality: Quality::default(),
            number_up: 1,
            number_up_layout: NumberUpLayout::default(),
            page_set: PageSet::default(),
            reverse: false,
            scale: DEFAULT_SCALE,
            media_type: None,
        }
    }
}

impl JobSettings {
    pub fn page_ranges(&self) -> Option<Vec<PageRange>> {
        match self.pages {
            PageSelection::Ranges => parse_ranges(&self.ranges),
            _ => None,
        }
    }

    // invalid input blocks printing instead of being silently corrected
    pub fn is_valid(&self) -> bool {
        let ranges_valid = self.pages != PageSelection::Ranges || self.page_ranges().is_some();

        ranges_valid && self.copies >= 1 && (MIN_SCALE..=MAX_SCALE).contains(&self.scale)
    }

    pub fn apply_portal(&mut self, settings: &PrintSettings, page_setup: &PageSetup) {
        let get = |key: &str| settings.get(key).map(String::as_str);

        let setup_name = match page_setup.get(setup::NAME) {
            Some(SetupValue::Text(name)) => Some(name.as_str()),
            _ => None,
        };

        let paper = get(key::PAPER_FORMAT)
            .and_then(Paper::from_pwg)
            .or_else(|| setup_name.and_then(Paper::from_pwg))
            .or_else(|| {
                let width = number(page_setup, setup::WIDTH)?;
                let height = number(page_setup, setup::HEIGHT)?;
                Paper::known_by_size(width, height)
            });

        if paper.is_some() {
            self.paper = paper;
        }

        let orientation = get(key::ORIENTATION).or(match page_setup.get(setup::ORIENTATION) {
            Some(SetupValue::Text(value)) => Some(value.as_str()),
            _ => None,
        });

        if let Some(orientation) = orientation.and_then(Orientation::from_portal) {
            self.orientation = orientation;
        }

        if let Some(copies) = get(key::N_COPIES).and_then(|v| v.parse::<u32>().ok()) {
            self.copies = copies.clamp(1, MAX_COPIES);
        }

        if let Some(collate) = get(key::COLLATE) {
            self.collate = collate == TRUE;
        }

        if let Some(reverse) = get(key::REVERSE) {
            self.reverse = reverse == TRUE;
        }

        if let Some(pages) = get(key::PRINT_PAGES).and_then(PageSelection::from_portal) {
            self.pages = pages;
        }

        if let Some(ranges) = get(key::PAGE_RANGES).and_then(ranges_from_portal) {
            self.ranges = format_ranges(ranges);
        }

        if let Some(duplex) = get(key::DUPLEX).and_then(Duplex::from_portal) {
            self.duplex = duplex;
        }

        if let Some(color) = get(key::USE_COLOR).and_then(ColorMode::from_portal) {
            self.color = color;
        }

        if let Some(quality) = get(key::QUALITY).and_then(Quality::from_portal) {
            self.quality = quality;
        }

        if let Some(number_up) = get(key::NUMBER_UP).and_then(|v| v.parse::<u32>().ok())
            && NUMBER_UP_CHOICES.contains(&number_up)
        {
            self.number_up = number_up;
        }

        if let Some(layout) = get(key::NUMBER_UP_LAYOUT).and_then(NumberUpLayout::from_portal) {
            self.number_up_layout = layout;
        }

        if let Some(page_set) = get(key::PAGE_SET).and_then(PageSet::from_portal) {
            self.page_set = page_set;
        }

        if let Some(scale) = get(key::SCALE)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|scale| scale.is_finite())
        {
            // Saturating float conversion preserves out-of-range portal input before clamping.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let scale = scale.round() as u32;
            self.scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        }

        if let Some(media_type) = get(key::MEDIA_TYPE) {
            self.media_type = Some(media_type.to_string());
        }
    }

    pub fn to_portal(&self, printer: Option<&str>, output: Option<&Path>) -> PrintSettings {
        let mut settings = PrintSettings::new();
        let mut set = |key: &str, value: String| {
            settings.insert(key.to_string(), value);
        };

        if let Some(paper) = &self.paper {
            set(key::PAPER_FORMAT, paper.name.clone());
            set(key::PAPER_WIDTH, paper.width_mm.to_string());
            set(key::PAPER_HEIGHT, paper.height_mm.to_string());
        }

        set(key::ORIENTATION, self.orientation.portal().to_string());
        set(key::N_COPIES, self.copies.to_string());
        set(key::COLLATE, self.collate.to_string());
        set(key::REVERSE, self.reverse.to_string());
        set(key::DUPLEX, self.duplex.portal().to_string());
        set(key::QUALITY, self.quality.portal().to_string());
        set(key::NUMBER_UP, self.number_up.to_string());
        set(
            key::NUMBER_UP_LAYOUT,
            self.number_up_layout.portal().to_string(),
        );
        set(key::PAGE_SET, self.page_set.portal().to_string());
        set(key::SCALE, self.scale.to_string());

        if self.color != ColorMode::Auto {
            set(key::USE_COLOR, self.color.portal().to_string());
        }

        match self.page_ranges() {
            Some(ranges) => {
                set(key::PRINT_PAGES, PageSelection::Ranges.portal().to_string());
                set(key::PAGE_RANGES, ranges_to_portal(&ranges));
            }
            None if self.pages == PageSelection::Ranges => {
                set(key::PRINT_PAGES, PageSelection::All.portal().to_string());
            }
            None => set(key::PRINT_PAGES, self.pages.portal().to_string()),
        }

        if let Some(media_type) = &self.media_type {
            set(key::MEDIA_TYPE, media_type.clone());
        }

        if let Some(printer) = printer {
            set(key::PRINTER, printer.to_string());
        }

        if let Some(output) = output {
            set(key::OUTPUT_FILE_FORMAT, PDF_FORMAT.to_string());
            if let Ok(uri) = Url::from_file_path(output) {
                set(key::OUTPUT_URI, uri.to_string());
            }
        }

        settings
    }

    // based on the application's page setup so its margins are kept
    pub fn page_setup(&self, base: &PageSetup) -> PageSetup {
        let mut setup = base.clone();
        let mut set = |key: &str, value: SetupValue| {
            setup.insert(key.to_string(), value);
        };

        if let Some(paper) = &self.paper {
            set(setup::NAME, SetupValue::Text(paper.name.clone()));
            set(
                setup::DISPLAY_NAME,
                SetupValue::Text(paper.display_name().to_string()),
            );
            set(setup::WIDTH, SetupValue::Number(paper.width_mm));
            set(setup::HEIGHT, SetupValue::Number(paper.height_mm));
        }

        set(
            setup::ORIENTATION,
            SetupValue::Text(self.orientation.portal().to_string()),
        );

        // a PPD name from the application would contradict the paper chosen here
        setup.remove(setup::PPD_NAME);

        setup
    }

    // Prerendered documents already have page selection, order and scaling applied.
    pub fn ipp_options(&self, prerendered: bool) -> Vec<(&'static str, String)> {
        let collate = if self.collate {
            ipp::COLLATED
        } else {
            ipp::UNCOLLATED
        };

        let number_up = (self.number_up > 1).then(|| self.number_up.to_string());
        let layout = number_up
            .as_ref()
            .map(|_| self.number_up_layout.ipp().to_string());

        let mut options = vec![
            (ipp::COPIES, Some(self.copies.to_string())),
            (ipp::SIDES, Some(self.duplex.ipp().to_string())),
            (ipp::COLOR_MODE, Some(self.color.ipp().to_string())),
            (ipp::QUALITY, Some(self.quality.ipp().to_string())),
            (ipp::DOCUMENT_HANDLING, Some(collate.to_string())),
            (
                ipp::MEDIA,
                self.paper.as_ref().map(|paper| paper.name.clone()),
            ),
            (ipp::MEDIA_TYPE, self.media_type.clone()),
            (ipp::NUMBER_UP, number_up),
            (ipp::NUMBER_UP_LAYOUT, layout),
        ];

        if !prerendered {
            options.extend([
                (ipp::ORIENTATION, Some(self.orientation.ipp().to_string())),
                (ipp::PAGE_RANGES, self.page_ranges().map(format_ranges)),
                (
                    ipp::PAGE_SET,
                    (self.page_set != PageSet::All).then(|| self.page_set.ipp().to_string()),
                ),
                (
                    ipp::OUTPUT_ORDER,
                    self.reverse.then(|| ipp::REVERSE_ORDER.to_string()),
                ),
                // CUPS' default only shrinks oversized pages, fitting matches what the preview shows
                (ipp::PRINT_SCALING, Some(ipp::SCALE_TO_FIT.to_string())),
            ]);
        }

        options
            .into_iter()
            .filter_map(|(key, value)| Some((key, value?)))
            .collect()
    }

    // Match CUPS: n-up first, then odd/even output sides, then reverse the sides.
    pub fn sides(&self, page_count: u32) -> Vec<Vec<u32>> {
        let pages: Vec<u32> = match self.page_ranges() {
            Some(ranges) => ranges
                .iter()
                .flat_map(|range| range.start..=range.end.min(page_count))
                .map(|page| page - 1)
                .collect(),
            None => (0..page_count).collect(),
        };

        // output sides are counted from 1, so the first one is odd
        let keep = |position: usize| match self.page_set {
            PageSet::All => true,
            PageSet::Odd => position.is_multiple_of(2),
            PageSet::Even => !position.is_multiple_of(2),
        };

        let mut sides: Vec<Vec<u32>> = pages
            .chunks(self.number_up.max(1) as usize)
            .enumerate()
            .filter(|(position, _)| keep(*position))
            .map(|(_, side)| side.to_vec())
            .collect();

        if self.reverse {
            sides.reverse();
        }

        sides
    }

    pub fn sheet_count(&self, page_count: u32) -> u32 {
        let sides = u32::try_from(self.sides(page_count).len()).unwrap_or(u32::MAX);

        let per_sheet = match self.duplex {
            Duplex::Off => 1,
            _ => 2,
        };

        sides.div_ceil(per_sheet) * self.copies
    }
}

fn number(setup: &PageSetup, key: &str) -> Option<f64> {
    match setup.get(key) {
        Some(SetupValue::Number(number)) => Some(*number),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_settings_follow_what_gtk_expects() {
        let mut settings = JobSettings {
            pages: PageSelection::Ranges,
            ranges: "1-3, 7".to_string(),
            ..Default::default()
        };

        // GTK aborts on unknown output formats, they must be lowercase
        let portal = settings.to_portal(None, Some(Path::new("/tmp/out.pdf")));
        assert_eq!(portal[key::OUTPUT_FILE_FORMAT], "pdf");
        assert_eq!(portal[key::PAGE_RANGES], "0-2,6");

        settings.ranges.clear();
        settings.apply_portal(&portal, &PageSetup::new());
        assert_eq!(settings.ranges, "1-3,7");

        let last = PageRange {
            start: u32::MAX,
            end: u32::MAX,
        };
        assert_eq!(ranges_from_portal("4294967294"), Some(vec![last]));
        assert_eq!(ranges_to_portal(&[last]), "4294967294");
    }

    #[test]
    fn falls_back_to_page_setup_dimensions_for_short_paper_names() {
        let mut settings = JobSettings::default();
        let portal = PrintSettings::from([(key::PAPER_FORMAT.to_string(), "iso_a4".to_string())]);
        let setup = PageSetup::from([
            (
                setup::NAME.to_string(),
                SetupValue::Text("iso_a4".to_string()),
            ),
            (setup::WIDTH.to_string(), SetupValue::Number(297.0)),
            (setup::HEIGHT.to_string(), SetupValue::Number(210.0)),
        ]);

        settings.apply_portal(&portal, &setup);
        assert_eq!(settings.paper.unwrap().name, "iso_a4_210x297mm");
    }

    #[test]
    fn rejects_invalid_ranges() {
        assert!(ranges_from_portal(&u32::MAX.to_string()).is_none());

        for value in [
            "",
            "1,",
            ",1",
            "1,,2",
            "3-1",
            "1-2-3",
            "a-b",
            "0-4294967295",
        ] {
            assert!(ranges_from_portal(value).is_none(), "{value:?}");
        }

        assert!(parse_ranges("3-1").is_none());
        assert!(parse_ranges("0").is_none());
        assert!(parse_ranges("a-b").is_none());
        assert!(parse_ranges("").is_none());
    }

    #[test]
    fn selects_pages_like_cups() {
        let settings = JobSettings {
            pages: PageSelection::Ranges,
            ranges: "2-5,9-20".to_string(),
            page_set: PageSet::Even,
            reverse: true,
            number_up: 2,
            ..Default::default()
        };

        // pages 2,3,4,5,9,10 on sides [2,3] [4,5] [9,10], even sides: [4,5], reversed sides
        assert_eq!(settings.sides(10), vec![vec![3, 4]]);

        let settings = JobSettings {
            reverse: true,
            number_up: 2,
            ..Default::default()
        };

        assert_eq!(settings.sides(4), vec![vec![2, 3], vec![0, 1]]);
    }

    #[test]
    fn counts_sheets() {
        let settings = JobSettings {
            pages: PageSelection::Ranges,
            ranges: "1-5,9-20".to_string(),
            duplex: Duplex::LongEdge,
            number_up: 2,
            copies: 2,
            ..Default::default()
        };

        // pages 1-5 and 9-10 of a 10 page document = 7 pages, 4 per sheet = 2 sheets, 2 copies
        assert_eq!(settings.sheet_count(10), 4);
    }

    #[test]
    fn prerendered_documents_only_receive_device_options() {
        let settings = JobSettings {
            pages: PageSelection::Ranges,
            ranges: "2".to_string(),
            reverse: true,
            ..Default::default()
        };

        let has = |prerendered, key: &str| {
            settings
                .ipp_options(prerendered)
                .into_iter()
                .any(|(k, _)| k == key)
        };

        assert!(has(false, ipp::PAGE_RANGES) && has(false, ipp::OUTPUT_ORDER));
        assert!(!has(true, ipp::PAGE_RANGES) && !has(true, ipp::OUTPUT_ORDER));
        assert!(has(true, ipp::COPIES));
    }
}
