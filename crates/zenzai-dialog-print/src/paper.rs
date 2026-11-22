use std::fmt;

const MM_PER_INCH: f64 = 25.4;
const SIZE_TOLERANCE_MM: f64 = 1.0;

// PWG name, the name GTK shows and a short alias for the command line,
// also offered when a printer reports no media
const KNOWN: [(&str, &str, &str); 16] = [
    ("iso_a3_297x420mm", "A3", "a3"),
    ("iso_a4_210x297mm", "A4", "a4"),
    ("iso_a5_148x210mm", "A5", "a5"),
    ("iso_a6_105x148mm", "A6", "a6"),
    ("iso_b4_250x353mm", "B4", "b4"),
    ("iso_b5_176x250mm", "B5", "b5"),
    ("jis_b5_182x257mm", "JIS B5", "jis-b5"),
    ("na_letter_8.5x11in", "US Letter", "letter"),
    ("na_legal_8.5x14in", "US Legal", "legal"),
    ("na_ledger_11x17in", "Tabloid", "tabloid"),
    ("na_executive_7.25x10.5in", "Executive", "executive"),
    ("na_invoice_5.5x8.5in", "Statement", "statement"),
    ("iso_c5_162x229mm", "C5 Envelope", "c5"),
    ("iso_dl_110x220mm", "DL Envelope", "dl"),
    ("na_number-10_4.125x9.5in", "#10 Envelope", "envelope-10"),
    ("na_index-4x6_4x6in", "Index 4×6", "4x6"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Paper {
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    display: String,
}

impl Paper {
    pub fn from_pwg(name: &str) -> Option<Self> {
        let (width_mm, height_mm) = pwg_size(name)?;

        Some(Self {
            display: display_name(name, (width_mm, height_mm)),
            name: name.to_string(),
            width_mm,
            height_mm,
        })
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.to_lowercase();
        let known = KNOWN.iter().find(|(_, _, alias)| *alias == value);

        Self::from_pwg(known.map_or(value.as_str(), |(name, _, _)| name))
    }

    pub fn known() -> Vec<Self> {
        KNOWN
            .iter()
            .filter_map(|(name, _, _)| Self::from_pwg(name))
            .collect()
    }

    pub fn known_by_size(width_mm: f64, height_mm: f64) -> Option<Self> {
        Self::known()
            .into_iter()
            .find(|paper| sizes_match((paper.width_mm, paper.height_mm), (width_mm, height_mm)))
    }

    // printers report the same size under many names, show the established one
    pub fn display_name(&self) -> &str {
        &self.display
    }

    pub fn same_size(&self, other: &Self) -> bool {
        sizes_match(
            (self.width_mm, self.height_mm),
            (other.width_mm, other.height_mm),
        )
    }
}

// "<class>_<size>_<width>x<height><unit>", names come from applications and printers,
// so sizes that can not be rendered are rejected
fn pwg_size(name: &str) -> Option<(f64, f64)> {
    let dimensions = name.rsplit('_').next()?;

    let (dimensions, factor) = match dimensions.strip_suffix("mm") {
        Some(dimensions) => (dimensions, 1.0),
        None => (dimensions.strip_suffix("in")?, MM_PER_INCH),
    };

    let (width, height) = dimensions.split_once('x')?;
    let width = width.parse::<f64>().ok()? * factor;
    let height = height.parse::<f64>().ok()? * factor;

    let valid = |size: f64| size.is_finite() && size > 0.0;

    (valid(width) && valid(height)).then_some((width, height))
}

fn sizes_match(a: (f64, f64), b: (f64, f64)) -> bool {
    let normalize = |(width, height): (f64, f64)| (width.min(height), width.max(height));
    let (a, b) = (normalize(a), normalize(b));

    (a.0 - b.0).abs() < SIZE_TOLERANCE_MM && (a.1 - b.1).abs() < SIZE_TOLERANCE_MM
}

fn display_name(name: &str, size: (f64, f64)) -> String {
    let short = name.split('_').nth(1).unwrap_or(name);

    let known = KNOWN
        .iter()
        .find(|(known, _, _)| pwg_size(known).is_some_and(|known| sizes_match(known, size)));

    // unknown sizes keep their dimensions visible, the name alone is often meaningless
    let Some((known, display, _)) = known else {
        return format!(
            "{} ({:.0} × {:.0} mm)",
            capitalize(&short.replace('-', " ")),
            size.0,
            size.1
        );
    };

    // keep variants apart, a printer may list "A4" and "A4 borderless" side by side
    let known_short = known.split('_').nth(1).unwrap_or_default();

    match short
        .strip_prefix(known_short)
        .map(|rest| rest.trim_matches('-'))
    {
        Some("") => display.to_string(),
        Some(variant) => format!("{display} ({})", variant.replace('-', " ")),
        None if *known == name => display.to_string(),
        None => format!("{display} ({})", short.replace('-', " ")),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();

    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl fmt::Display for Paper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pwg_names_and_aliases() {
        let a4 = Paper::parse("A4").unwrap();
        assert_eq!((a4.width_mm, a4.height_mm), (210.0, 297.0));

        let letter = Paper::from_pwg("na_letter_8.5x11in").unwrap();
        assert!((letter.width_mm - 215.9).abs() < 0.01);

        assert!(Paper::from_pwg("Letter").is_none());
        assert!(Paper::from_pwg("x_0x10mm").is_none());
        assert!(Paper::from_pwg("x_infxinfmm").is_none());
    }

    #[test]
    fn printer_specific_names_get_established_display_names() {
        let name = |pwg| Paper::from_pwg(pwg).unwrap().display_name().to_string();

        assert_eq!(name("iso_a4_210x297mm"), "A4");
        assert_eq!(name("na_letter_8.5x11in"), "US Letter");
        assert_eq!(name("om_a4-borderless_210x297mm"), "A4 (borderless)");
        assert_eq!(name("na_index-5x8_5x8in"), "Index 5x8 (127 × 203 mm)");
    }
}
